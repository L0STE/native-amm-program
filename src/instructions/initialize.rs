use crate::state::Config;
use core::mem::{size_of, MaybeUninit};
use pinocchio::{
    account_info::AccountInfo,
    instruction::{Seed, Signer},
    program_error::ProgramError,
    sysvars::{rent::Rent, Sysvar},
    ProgramResult,
};
use pinocchio_system::instructions::CreateAccount;
use pinocchio_token::{instructions::InitializeMint2, state::Mint};

/// #Initialize
///
/// Initialize the Amm
///
/// Accounts:
///
/// 1. initializer:                 [signer, mut]
/// 4. mint_lp:                     [init]
/// 5. vault_x                      [init]
/// 6. vault_y                      [init]
/// 7. authority                    
/// 8. config                       [init]
/// 9. system_program               [executable]
/// 10. token_program                [executable]
/// 11. associated_token_program     [executable]
///
/// Parameters:
///
/// 1. seed:          [u64]
/// 2. fee:           [u16]
/// 3. authority:     [Option<Pubkey>]
pub struct InitializeAccounts<'a> {
    pub initializer: &'a AccountInfo,
    pub mint_lp: &'a AccountInfo,
    pub config: &'a AccountInfo,
}

impl<'a> TryFrom<&'a [AccountInfo]> for InitializeAccounts<'a> {
    type Error = ProgramError;

    fn try_from(accounts: &'a [AccountInfo]) -> Result<Self, Self::Error> {
        let [initializer, mint_lp, config, _system_program, _token_program] = accounts else {
            return Err(ProgramError::NotEnoughAccountKeys);
        };

        // Return the accounts
        Ok(Self {
            initializer,
            mint_lp,
            config,
        })
    }
}

#[repr(C, packed)]
pub struct InitializeInstructionData {
    pub seed: u64,
    pub fee: u16,
    pub mint_x: [u8; 32],
    pub mint_y: [u8; 32],
    pub config_bump: [u8; 1],
    pub lp_bump: [u8; 1],
    pub authority: [u8; 32],
}

impl TryFrom<&[u8]> for InitializeInstructionData {
    type Error = ProgramError;

    fn try_from(data: &[u8]) -> Result<Self, Self::Error> {
        const INITIALIZE_DATA_LEN_WITH_AUTHORITY: usize = size_of::<InitializeInstructionData>();
        const INITIALIZE_DATA_LEN: usize =
            INITIALIZE_DATA_LEN_WITH_AUTHORITY - size_of::<[u8; 32]>();

        match data.len() {
            INITIALIZE_DATA_LEN_WITH_AUTHORITY => {
                Ok(unsafe { (data.as_ptr() as *const Self).read_unaligned() })
            }
            INITIALIZE_DATA_LEN => {
                // If the authority is not present, we need to build the buffer and add it at the end before transmuting to the struct
                let mut raw: MaybeUninit<[u8; INITIALIZE_DATA_LEN]> = MaybeUninit::uninit();
                let raw_ptr = raw.as_mut_ptr() as *mut u8;
                unsafe {
                    // Copy the provided data
                    core::ptr::copy_nonoverlapping(data.as_ptr(), raw_ptr, INITIALIZE_DATA_LEN);
                    // Add the authority to the end of the buffer
                    core::ptr::write_bytes(raw_ptr.add(INITIALIZE_DATA_LEN), 0, 32);
                    // Now transmute to the struct
                    Ok((raw.as_ptr() as *const Self).read_unaligned())
                }
            }
            _ => Err(ProgramError::InvalidInstructionData),
        }
    }
}

pub struct Initialize<'a> {
    pub accounts: InitializeAccounts<'a>,
    pub instruction_data: InitializeInstructionData,
}

impl<'a> TryFrom<(&'a [u8], &'a [AccountInfo])> for Initialize<'a> {
    type Error = ProgramError;

    fn try_from((data, accounts): (&'a [u8], &'a [AccountInfo])) -> Result<Self, Self::Error> {
        let accounts = InitializeAccounts::try_from(accounts)?;
        let instruction_data = InitializeInstructionData::try_from(data)?;

        Ok(Self {
            accounts,
            instruction_data,
        })
    }
}

impl<'a> Initialize<'a> {
    pub const DISCRIMINATOR: &'a u8 = &0;

    pub fn process(&mut self) -> ProgramResult {
        // Create the config account
        let seed_binding = self.instruction_data.seed.to_le_bytes();
        let config_seeds = [
            Seed::from(b"config"),
            Seed::from(&seed_binding),
            Seed::from(&self.instruction_data.mint_x),
            Seed::from(&self.instruction_data.mint_y),
            Seed::from(&self.instruction_data.config_bump),
        ];

        let config_lamports = Rent::get()?.minimum_balance(Config::LEN);

        CreateAccount {
            from: self.accounts.initializer,
            to: self.accounts.config,
            lamports: config_lamports,
            space: Config::LEN as u64,
            owner: &crate::ID,
        }
        .invoke_signed(&[Signer::from(&config_seeds)])?;

        let config = unsafe {
            Config::load_mut_unchecked(self.accounts.config.borrow_mut_data_unchecked())
        }?;

        config.set_inner(
            self.instruction_data.seed,
            self.instruction_data.authority,
            self.instruction_data.mint_x,
            self.instruction_data.mint_y,
            self.instruction_data.fee,
            self.instruction_data.config_bump,
        );

        // Create the mint_lp account
        let mint_lp_seeds = [
            Seed::from(b"mint_lp"),
            Seed::from(self.accounts.config.key()),
            Seed::from(&self.instruction_data.lp_bump),
        ];

        let mint_size = Mint::LEN;
        let mint_lamports = Rent::get()?.minimum_balance(mint_size);

        CreateAccount {
            from: self.accounts.initializer,
            to: self.accounts.mint_lp,
            lamports: mint_lamports,
            space: mint_size as u64,
            owner: &crate::ID,
        }
        .invoke_signed(&[Signer::from(&mint_lp_seeds)])?;

        InitializeMint2 {
            mint: self.accounts.mint_lp,
            decimals: 6,
            mint_authority: self.accounts.config.key(),
            freeze_authority: None,
        }
        .invoke_signed(&[Signer::from(&mint_lp_seeds)])?;

        Ok(())
    }
}
