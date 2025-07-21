use crate::state::Config;
use crate::AmmState;
use constant_product_curve::ConstantProduct;
use core::mem::size_of;
use pinocchio::instruction::{Seed, Signer};
use pinocchio::pubkey::find_program_address;
use pinocchio::sysvars::clock::Clock;
use pinocchio::sysvars::Sysvar;
use pinocchio::{account_info::AccountInfo, program_error::ProgramError, ProgramResult};
use pinocchio_token::instructions::{MintTo, Transfer};
use pinocchio_token::state::{Mint, TokenAccount};

/// #Deposit
///
/// Deposit tokens into the Amm
///
/// Accounts:
///
/// 1. user:                         [signer, mut]
/// 2. mint_lp                      [mut]
/// 3. vault_x                      [mut]
/// 4. vault_y                      [mut]
/// 5. user_x_ata                   [mut]
/// 6. user_y_ata                   [mut]
/// 7. user_lp_ata                  [init_if_needed]
/// 8. config                       
/// 9. system_program               [executable]
/// 10. token_program                [executable]
///
/// Parameters:
///
/// 1. amount: u64,        // Amount of LP token to claim
/// 2. max_x: u64,         // Max amount of X we are willing to deposit
/// 3. max_y: u64,         // Max amount of Y we are willing to deposit
/// 4. expiration: i64     // Expiration of the offer
pub struct DepositAccounts<'a> {
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

impl<'a> TryFrom<&'a [AccountInfo]> for DepositAccounts<'a> {
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
pub struct DepositInstructionData {
    pub amount: u64,
    pub max_x: u64,
    pub max_y: u64,
    pub expiration: i64,
}

impl<'a> TryFrom<&'a [u8]> for DepositInstructionData {
    type Error = ProgramError;

    fn try_from(data: &'a [u8]) -> Result<Self, Self::Error> {
        if data.len().ne(&size_of::<DepositInstructionData>()) {
            return Err(ProgramError::InvalidInstructionData);
        }

        // This is safe because we checked the length and the struct is packed
        let raw = unsafe { (data.as_ptr() as *const DepositInstructionData).read_unaligned() };

        if raw.amount == 0
            || raw.max_x == 0
            || raw.max_y == 0
            || raw.expiration < Clock::get()?.unix_timestamp
        {
            return Err(ProgramError::InvalidInstructionData);
        }

        Ok(Self {
            amount: u64::from_le(raw.amount),
            max_x: u64::from_le(raw.max_x),
            max_y: u64::from_le(raw.max_y),
            expiration: i64::from_le(raw.expiration),
        })
    }
}

pub struct Deposit<'a> {
    pub accounts: DepositAccounts<'a>,
    pub instruction_data: DepositInstructionData,
}

impl<'a> TryFrom<(&'a [u8], &'a [AccountInfo])> for Deposit<'a> {
    type Error = ProgramError;

    fn try_from((data, accounts): (&'a [u8], &'a [AccountInfo])) -> Result<Self, Self::Error> {
        let accounts = DepositAccounts::try_from(accounts)?;
        let instruction_data = DepositInstructionData::try_from(data)?;

        // Return the initialized struct
        Ok(Self {
            accounts,
            instruction_data,
        })
    }
}

impl<'a> Deposit<'a> {
    pub const DISCRIMINATOR: &'a u8 = &1;

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

        // Grab the amounts to deposit
        let (x, y) = match mint_lp.supply() == 0 && vault_x.amount() == 0 && vault_y.amount() == 0 {
            true => (self.instruction_data.max_x, self.instruction_data.max_y),
            false => {
                let amounts = ConstantProduct::xy_deposit_amounts_from_l(
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
        if !(x <= self.instruction_data.max_x && y <= self.instruction_data.max_y) {
            return Err(ProgramError::InvalidArgument);
        }

        // Create the seeds
        let seeds_binding = config.seed.to_le_bytes();

        let seeds = [
            Seed::from(b"config"),
            Seed::from(&seeds_binding),
            Seed::from(&config.mint_x),
            Seed::from(&config.mint_y),
            Seed::from(&config.config_bump),
        ];

        // Transfer the tokens to the vault
        Transfer {
            from: self.accounts.user_x_ata,
            to: self.accounts.vault_x,
            authority: self.accounts.user,
            amount: x,
        }
        .invoke()?;

        Transfer {
            from: self.accounts.user_y_ata,
            to: self.accounts.vault_y,
            authority: self.accounts.user,
            amount: y,
        }
        .invoke()?;

        // Mint the LP tokens to the user
        MintTo {
            mint: self.accounts.mint_lp,
            account: self.accounts.user_lp_ata,
            mint_authority: self.accounts.config,
            amount: self.instruction_data.amount,
        }
        .invoke_signed(&[Signer::from(&seeds)])?;

        Ok(())
    }
}
