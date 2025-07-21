use crate::state::Config;
use crate::AmmState;
use constant_product_curve::{ConstantProduct, LiquidityPair};
use pinocchio::log::sol_log_64;
use core::mem::size_of;
use pinocchio::instruction::Signer;
use pinocchio::pubkey::find_program_address;
use pinocchio::sysvars::clock::Clock;
use pinocchio::sysvars::Sysvar;
use pinocchio::{
    account_info::AccountInfo, instruction::Seed, program_error::ProgramError, ProgramResult,
};
use pinocchio_token::instructions::Transfer;
use pinocchio_token::state::TokenAccount;

/// #Swap
///
/// Swap from Token X to Token Y or vice versa
///
/// Accounts:
///
/// 1. user:                        [signer, mut]
/// 2. user_x:                      [init_if_needed]
/// 3. user_y:                      [init_if_needed]
/// 4. vault_x                      [mut]
/// 5. vault_y                      [mut]
/// 6. config                       
/// 7. token_program               [executable]
///
/// Parameters:
///
/// 1. is_x:                        [bool]
/// 2. amount:                      [u64]
/// 3. min:                         [u64]
/// 4. expiration:                  [u64]
pub struct SwapAccounts<'a> {
    pub user: &'a AccountInfo,
    pub user_x: &'a AccountInfo,
    pub user_y: &'a AccountInfo,
    pub vault_x: &'a AccountInfo,
    pub vault_y: &'a AccountInfo,
    pub config: &'a AccountInfo,
    pub token_program: &'a AccountInfo,
}

impl<'a> TryFrom<&'a [AccountInfo]> for SwapAccounts<'a> {
    type Error = ProgramError;

    fn try_from(accounts: &'a [AccountInfo]) -> Result<Self, Self::Error> {
        let [user, user_x, user_y, vault_x, vault_y, config, token_program] = accounts else {
            return Err(ProgramError::NotEnoughAccountKeys);
        };

        // Return the accounts
        Ok(Self {
            user,
            user_x,
            user_y,
            vault_x,
            vault_y,
            config,
            token_program,
        })
    }
}

#[repr(C, packed)]
pub struct SwapInstructionData {
    pub is_x: bool,
    pub amount: u64,
    pub min: u64,
    pub expiration: i64,
}

impl<'a> TryFrom<&'a [u8]> for SwapInstructionData {
    type Error = ProgramError;

    fn try_from(data: &'a [u8]) -> Result<Self, Self::Error> {
        if data.len().ne(&size_of::<SwapInstructionData>()) {
            return Err(ProgramError::InvalidInstructionData);
        }

        // This is safe because we checked the length and the struct is packed
        let raw = unsafe { (data.as_ptr() as *const SwapInstructionData).read_unaligned() };

        if raw.amount == 0 || raw.min == 0 || raw.expiration < Clock::get()?.unix_timestamp {
            return Err(ProgramError::InvalidInstructionData);
        }

        Ok(Self {
            is_x: raw.is_x,
            amount: u64::from_le(raw.amount),
            min: u64::from_le(raw.min),
            expiration: i64::from_le(raw.expiration),
        })
    }
}

pub struct Swap<'a> {
    pub accounts: SwapAccounts<'a>,
    pub instruction_data: SwapInstructionData,
}

impl<'a> TryFrom<(&'a [u8], &'a [AccountInfo])> for Swap<'a> {
    type Error = ProgramError;

    fn try_from((data, accounts): (&'a [u8], &'a [AccountInfo])) -> Result<Self, Self::Error> {
        let accounts = SwapAccounts::try_from(accounts)?;
        let instruction_data = SwapInstructionData::try_from(data)?;

        sol_log_64(2, 0, 0, 0, 0);

        // Return the initialized struct
        Ok(Self {
            accounts,
            instruction_data,
        })
    }
}
impl<'a> Swap<'a> {
    pub const DISCRIMINATOR: &'a u8 = &3;

    pub fn process(&mut self) -> ProgramResult {
        // Deserialize the config account
        let config = Config::load(self.accounts.config)?;

        // Check if we can swap in the Amm
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
        let vault_x = unsafe { TokenAccount::from_account_info_unchecked(self.accounts.vault_x)? };
        let vault_y = unsafe { TokenAccount::from_account_info_unchecked(self.accounts.vault_y)? };

        // Swap Calculations
        let mut curve = ConstantProduct::init(
            vault_x.amount(),
            vault_y.amount(),
            vault_x.amount(),
            config.fee,
            None,
        )
        .map_err(|_| ProgramError::Custom(1))?;

        let p = match self.instruction_data.is_x {
            true => LiquidityPair::X,
            false => LiquidityPair::Y,
        };

        let swap_result = curve
            .swap(p, self.instruction_data.amount, self.instruction_data.min)
            .map_err(|_| ProgramError::Custom(1))?;

        // Check for correct values
        if swap_result.deposit == 0 || swap_result.withdraw == 0 {
            return Err(ProgramError::InvalidArgument);
        }

        // Create the signer seeds
        let seed_binding = config.seed.to_le_bytes();
        let seeds = [
            Seed::from("config".as_bytes()),
            Seed::from(&seed_binding),
            Seed::from(&config.mint_x),
            Seed::from(&config.mint_y),
            Seed::from(&config.config_bump),
        ];
        let signer_seeds = [Signer::from(&seeds)];

        // Swap the tokens
        if self.instruction_data.is_x {
            Transfer {
                from: self.accounts.user_x,
                to: self.accounts.vault_x,
                authority: self.accounts.user,
                amount: swap_result.deposit,
            }
            .invoke()?;

            Transfer {
                from: self.accounts.vault_y,
                to: self.accounts.user_y,
                authority: self.accounts.config,
                amount: swap_result.withdraw,
            }
            .invoke_signed(&signer_seeds)?;
        } else {
            Transfer {
                from: self.accounts.user_y,
                to: self.accounts.vault_y,
                authority: self.accounts.user,
                amount: swap_result.deposit,
            }
            .invoke()?;

            Transfer {
                from: self.accounts.vault_x,
                to: self.accounts.user_x,
                authority: self.accounts.config,
                amount: swap_result.withdraw,
            }
            .invoke_signed(&signer_seeds)?;
        }

        Ok(())
    }
}
