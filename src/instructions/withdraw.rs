use crate::state::Config;
use crate::AmmState;
use constant_product_curve::ConstantProduct;
use core::mem::size_of;
use pinocchio::instruction::{Seed, Signer};
use pinocchio::pubkey::find_program_address;
use pinocchio::sysvars::clock::Clock;
use pinocchio::sysvars::Sysvar;
use pinocchio::{account_info::AccountInfo, program_error::ProgramError, ProgramResult};
use pinocchio_token::instructions::{Burn, Transfer};
use pinocchio_token::state::{Mint, TokenAccount};

/// #Withdraw
///
/// Withdraw tokens from the Amm
///
/// Accounts:
///
/// 1. user:                        [signer, mut]   
/// 2. mint_lp                      [mut]
/// 3. vault_x                      [mut]
/// 4. vault_y                      [mut]
/// 5. user_x_ata                   [init_if_needed]
/// 6. user_y_ata                   [init_if_needed]
/// 7. user_lp_ata                  [mut]
/// 8. config                       
/// 9. token_program                [executable]
///
/// Parameters:
///
/// 1. amount: u64,        // Amount of LP token to claim
/// 2. min_x: u64,         // Min amount of X we are willing to receive
/// 3. min_y: u64,         // Min amount of Y we are willing to receive
/// 4. expiration: i64     // Expiration of the offer
pub struct WithdrawAccounts<'a> {
    pub user: &'a AccountInfo,
    pub mint_lp: &'a AccountInfo,
    pub vault_x: &'a AccountInfo,
    pub vault_y: &'a AccountInfo,
    pub user_x_ata: &'a AccountInfo,
    pub user_y_ata: &'a AccountInfo,
    pub user_lp_ata: &'a AccountInfo,
    pub config: &'a AccountInfo,
    pub token_program: &'a AccountInfo,
}

impl<'a> TryFrom<&'a [AccountInfo]> for WithdrawAccounts<'a> {
    type Error = ProgramError;

    fn try_from(accounts: &'a [AccountInfo]) -> Result<Self, Self::Error> {
        let [user, mint_lp, vault_x, vault_y, user_x_ata, user_y_ata, user_lp_ata, config, token_program] =
            accounts
        else {
            return Err(ProgramError::NotEnoughAccountKeys);
        };

        // Return the accounts
        Ok(Self {
            user,
            mint_lp,
            vault_x,
            vault_y,
            user_x_ata,
            user_y_ata,
            user_lp_ata,
            config,
            token_program,
        })
    }
}

#[repr(C, packed)]
pub struct WithdrawInstructionData {
    pub amount: u64,
    pub min_x: u64,
    pub min_y: u64,
    pub expiration: i64,
}

impl<'a> TryFrom<&'a [u8]> for WithdrawInstructionData {
    type Error = ProgramError;

    fn try_from(data: &'a [u8]) -> Result<Self, Self::Error> {
        if data.len().ne(&size_of::<WithdrawInstructionData>()) {
            return Err(ProgramError::InvalidInstructionData);
        }

        // This is safe because we checked the length and the struct is packed
        let raw = unsafe { (data.as_ptr() as *const WithdrawInstructionData).read_unaligned() };

        if raw.amount == 0
            || raw.min_x == 0
            || raw.min_y == 0
            || raw.expiration < Clock::get()?.unix_timestamp
        {
            return Err(ProgramError::InvalidInstructionData);
        }

        Ok(Self {
            amount: u64::from_le(raw.amount),
            min_x: u64::from_le(raw.min_x),
            min_y: u64::from_le(raw.min_y),
            expiration: i64::from_le(raw.expiration),
        })
    }
}

pub struct Withdraw<'a> {
    pub accounts: WithdrawAccounts<'a>,
    pub instruction_data: WithdrawInstructionData,
}

impl<'a> TryFrom<(&'a [u8], &'a [AccountInfo])> for Withdraw<'a> {
    type Error = ProgramError;

    fn try_from((data, accounts): (&'a [u8], &'a [AccountInfo])) -> Result<Self, Self::Error> {
        let accounts = WithdrawAccounts::try_from(accounts)?;
        let instruction_data = WithdrawInstructionData::try_from(data)?;

        // Return the initialized struct
        Ok(Self {
            accounts,
            instruction_data,
        })
    }
}

impl<'a> Withdraw<'a> {
    pub const DISCRIMINATOR: &'a u8 = &2;

    pub fn process(&mut self) -> ProgramResult {
        // Deserialize the config account
        let config = Config::load(self.accounts.config)?;

        // Check if we can deposit to the Amm
        if config.state.ne(&(AmmState::Initialized as u8)) {
            return Err(ProgramError::InvalidAccountData);
        }

        // Check if the vault_x is valid
        let (vault_x, _) = find_program_address(
            &[
                self.accounts.config.key(),
                self.accounts.token_program.key(),
                &config.mint_x,
            ],
            &pinocchio_associated_token_account::ID,
        );

        if vault_x.ne(self.accounts.vault_x.key()) {
            return Err(ProgramError::InvalidAccountData);
        }

        // Check if the vault_y is valid
        let (vault_y, _) = find_program_address(
            &[
                self.accounts.config.key(),
                self.accounts.token_program.key(),
                &config.mint_y,
            ],
            &pinocchio_associated_token_account::ID,
        );

        if vault_y.ne(self.accounts.vault_y.key()) {
            return Err(ProgramError::InvalidAccountData);
        }

        // Deserialize the token accounts
        let mint_lp = unsafe { Mint::from_account_info_unchecked(self.accounts.mint_lp)? };
        let vault_x = unsafe { TokenAccount::from_account_info_unchecked(self.accounts.vault_x)? };
        let vault_y = unsafe { TokenAccount::from_account_info_unchecked(self.accounts.vault_y)? };

        let (x, y) = match mint_lp.supply() == self.instruction_data.amount {
            true => (vault_x.amount(), vault_y.amount()),
            false => {
                let amounts = ConstantProduct::xy_withdraw_amounts_from_l(
                    vault_x.amount(),
                    vault_y.amount(),
                    mint_lp.supply(),
                    self.instruction_data.amount,
                    6,
                )
                .map_err(|_| ProgramError::InvalidArgument)?;

                (amounts.x, amounts.y)
            }
        };

        // Check for slippage
        if !(x <= self.instruction_data.min_x && y <= self.instruction_data.min_y) {
            return Err(ProgramError::InvalidArgument);
        }

        let seed_binding = config.seed.to_le_bytes();
        let seeds = [
            Seed::from("config".as_bytes()),
            Seed::from(&seed_binding),
            Seed::from(&config.mint_x),
            Seed::from(&config.mint_y),
            Seed::from(&config.config_bump),
        ];
        let signer_seeds = [Signer::from(&seeds)];

        Transfer {
            from: self.accounts.vault_x,
            to: self.accounts.user_x_ata,
            authority: self.accounts.config,
            amount: x,
        }
        .invoke_signed(&signer_seeds)?;

        Transfer {
            from: self.accounts.vault_y,
            to: self.accounts.user_y_ata,
            authority: self.accounts.config,
            amount: y,
        }
        .invoke_signed(&signer_seeds)?;

        Burn {
            mint: self.accounts.mint_lp,
            account: self.accounts.user_lp_ata,
            authority: self.accounts.user,
            amount: self.instruction_data.amount,
        }
        .invoke()?;

        Ok(())
    }
}
