use crate::state::Config;
use core::mem::size_of;
use pinocchio::{account_info::AccountInfo, program_error::ProgramError, ProgramResult};

/// #UpdateConfig
///
/// Update the Amm Config Account
///
/// Accounts:
///
/// 1. authority:                 [signer]
/// 2. config:                      [mut]
///
pub struct UpdateConfigAccounts<'a> {
    pub authority: &'a AccountInfo,
    pub config: &'a AccountInfo,
}

impl<'a> TryFrom<&'a [AccountInfo]> for UpdateConfigAccounts<'a> {
    type Error = ProgramError;

    fn try_from(accounts: &'a [AccountInfo]) -> Result<Self, Self::Error> {
        let [authority, config] = accounts else {
            return Err(ProgramError::NotEnoughAccountKeys);
        };

        // Deserialize the config account
        let config_data = Config::load(config)?;

        // Check if the authority is the correct authority
        if config_data.has_authority().ne(&Some(*authority.key())) {
            return Err(ProgramError::InvalidAccountData);
        }

        // Check if the authority has signed the transaction
        if !authority.is_signer() {
            return Err(ProgramError::InvalidAccountData);
        }

        // Return the accounts
        Ok(Self { authority, config })
    }
}

pub struct UpdateConfigAuthorityInstructionData {
    pub authority: [u8; 32],
}

impl<'a> TryFrom<&'a [u8]> for UpdateConfigAuthorityInstructionData {
    type Error = ProgramError;

    fn try_from(data: &'a [u8]) -> Result<Self, Self::Error> {
        Ok(Self {
            authority: data.try_into().unwrap(),
        })
    }
}

pub struct UpdateConfigFeeInstructionData {
    pub fee: u16,
}

impl<'a> TryFrom<&'a [u8]> for UpdateConfigFeeInstructionData {
    type Error = ProgramError;

    fn try_from(data: &'a [u8]) -> Result<Self, Self::Error> {
        Ok(Self {
            fee: u16::from_le_bytes(data.try_into().unwrap()),
        })
    }
}

pub struct UpdateConfigStatusInstructionData {
    pub status: u8,
}

impl<'a> TryFrom<&'a [u8]> for UpdateConfigStatusInstructionData {
    type Error = ProgramError;

    fn try_from(data: &'a [u8]) -> Result<Self, Self::Error> {
        Ok(Self { status: data[0] })
    }
}

pub struct UpdateConfig<'a> {
    pub accounts: UpdateConfigAccounts<'a>,
    pub data: &'a [u8],
}

impl<'a> TryFrom<(&'a [u8], &'a [AccountInfo])> for UpdateConfig<'a> {
    type Error = ProgramError;

    fn try_from((data, accounts): (&'a [u8], &'a [AccountInfo])) -> Result<Self, Self::Error> {
        let accounts = UpdateConfigAccounts::try_from(accounts)?;

        // Return the initialized struct
        Ok(Self { accounts, data })
    }
}

impl<'a> UpdateConfig<'a> {
    pub const DISCRIMINATOR: &'a u8 = &4;

    pub fn process(&mut self) -> ProgramResult {
        match self.data.len() {
            len if len == size_of::<UpdateConfigStatusInstructionData>() => {
                self.process_update_status()
            }
            len if len == size_of::<UpdateConfigFeeInstructionData>() => self.process_update_fee(),
            len if len == size_of::<UpdateConfigAuthorityInstructionData>() => {
                self.process_update_authority()
            }
            _ => Err(ProgramError::InvalidInstructionData),
        }
    }

    pub fn process_update_authority(&mut self) -> ProgramResult {
        let instruction_data = UpdateConfigAuthorityInstructionData::try_from(self.data)?;

        let mut config = Config::load_mut(self.accounts.config)?;

        config.set_authority(instruction_data.authority);

        Ok(())
    }

    pub fn process_update_fee(&mut self) -> ProgramResult {
        let instruction_data = UpdateConfigFeeInstructionData::try_from(self.data)?;

        let mut config = Config::load_mut(self.accounts.config)?;

        config.set_fee(instruction_data.fee)?;

        Ok(())
    }

    pub fn process_update_status(&mut self) -> ProgramResult {
        let instruction_data = UpdateConfigStatusInstructionData::try_from(self.data)?;

        let mut config = Config::load_mut(self.accounts.config)?;

        config.set_state(instruction_data.status)?;

        Ok(())
    }
}
