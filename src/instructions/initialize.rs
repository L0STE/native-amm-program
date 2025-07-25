use crate::state::Config;
use core::mem::size_of;
use pinocchio::{
    account_info::AccountInfo, instruction::{Seed, Signer}, program_error::ProgramError, sysvars::{rent::Rent, Sysvar}, ProgramResult
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
/// 2. mint_lp:                     [init]
/// 3. config                       [init]
/// 4. system_program               [executable]
/// 5. token_program                [executable]
///
/// Parameters:
///
/// 1. seed:          [u64]
/// 2. fee:           [u16]
/// 3. mint_x:        [Pubkey]
/// 4. mint_y:        [Pubkey]
/// 5. config_bump:   [u8]
/// 6. lp_bump:       [u8]
/// 5. authority:     [Option<Pubkey>]
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

pub struct InitializeInstructionData {
    pub seed: u64,
    pub fee: u16,
    pub mint_x: [u8; 32],
    pub mint_y: [u8; 32],
    pub config_bump: [u8; 1],
    pub lp_bump: [u8; 1],
    pub authority: Option<[u8; 32]>,
}

impl TryFrom<&[u8]> for InitializeInstructionData {
    type Error = ProgramError;

    fn try_from(data: &[u8]) -> Result<Self, Self::Error> {
        const INITIALIZE_DATA_LEN_WITHOUT_AUTHORITY: usize = size_of::<u64>() + size_of::<u16>() + size_of::<[u8; 32]>() * 2 + size_of::<[u8; 1]>() * 2;
        const INITIALIZE_DATA_LEN_WITH_AUTHORITY: usize = INITIALIZE_DATA_LEN_WITHOUT_AUTHORITY + size_of::<[u8; 32]>();

        match data.len() {
            INITIALIZE_DATA_LEN_WITH_AUTHORITY => {
                let seed = u64::from_le_bytes(data[0..8].try_into().unwrap());
                let fee = u16::from_le_bytes(data[8..10].try_into().unwrap());
                let mint_x = data[10..42].try_into().unwrap();
                let mint_y = data[42..74].try_into().unwrap();
                let config_bump: [u8; 1] = data[74..75].try_into().unwrap();
                let lp_bump: [u8; 1] = data[75..76].try_into().unwrap();
                let authority = data[76..108].try_into().unwrap();

                Ok(Self {
                    seed,
                    fee,
                    mint_x,
                    mint_y,
                    config_bump,
                    lp_bump,
                    authority: Some(authority),
                })
            }
            INITIALIZE_DATA_LEN => {
                let seed = u64::from_le_bytes(data[0..8].try_into().unwrap());
                let fee = u16::from_le_bytes(data[8..10].try_into().unwrap());
                let mint_x = data[10..42].try_into().unwrap();
                let mint_y = data[42..74].try_into().unwrap();
                let config_bump: [u8; 1] = data[74..75].try_into().unwrap();
                let lp_bump: [u8; 1] = data[75..76].try_into().unwrap();

                Ok(Self {
                    seed,
                    fee,
                    mint_x,
                    mint_y,
                    config_bump,
                    lp_bump,
                    authority: None,
                })
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
        let instruction_data: InitializeInstructionData = InitializeInstructionData::try_from(data)?;

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
            Config::load_mut_unchecked(self.accounts.config)
        }?;

        config.set_inner(
            self.instruction_data.seed,
            self.instruction_data.authority.unwrap_or_default(),
            self.instruction_data.mint_x,
            self.instruction_data.mint_y,
            self.instruction_data.fee,
            self.instruction_data.config_bump,
        )?;

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
            owner: &pinocchio_token::ID,
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
