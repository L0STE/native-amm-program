use core::mem::size_of;
use pinocchio::{account_info::AccountInfo, program_error::ProgramError, pubkey::Pubkey};

#[repr(C, packed)]
pub struct Config {
    pub state: u8,
    pub seed: u64,
    pub authority: Pubkey,
    pub mint_x: Pubkey, // Token X Mint
    pub mint_y: Pubkey, // Token Y Mint
    pub fee: u16,       // Swap fee in basis points
    pub config_bump: [u8; 1],
}

#[repr(u8)]
pub enum AmmState {
    Uninitialized = 0u8,
    Initialized = 1u8,
    Disabled = 2u8,
    WithdrawOnly = 3u8,
}

impl Config {
    pub const LEN: usize = size_of::<u8>()
        + size_of::<u64>()
        + size_of::<Pubkey>() * 3
        + size_of::<u16>()
        + size_of::<u8>();

    #[inline(always)]
    pub fn load(account_info: &AccountInfo) -> Result<&Self, ProgramError> {
        if account_info.owner().ne(&crate::ID) {
            return Err(ProgramError::InvalidAccountData);
        }

        let data = account_info.try_borrow_data()?;

        if data.len().ne(&Config::LEN) {
            return Err(ProgramError::InvalidAccountData);
        }

        Ok(unsafe { &*(data.as_ptr() as *const Self) })
    }

    #[inline(always)]
    pub unsafe fn load_unchecked(bytes: &mut [u8]) -> Result<&Self, ProgramError> {
        if bytes.len().ne(&Config::LEN) {
            return Err(ProgramError::InvalidAccountData);
        }

        Ok(&*(bytes.as_ptr() as *const Self))
    }

    #[inline(always)]
    pub fn load_mut_unchecked(bytes: &mut [u8]) -> Result<&mut Self, ProgramError> {
        if bytes.len().ne(&Config::LEN) {
            return Err(ProgramError::InvalidAccountData);
        }

        Ok(unsafe { &mut *(bytes.as_mut_ptr() as *mut Self) })
    }

    #[inline(always)]
    pub fn set_inner(
        &mut self,
        seed: u64,
        authority: Pubkey,
        mint_x: Pubkey,
        mint_y: Pubkey,
        fee: u16,
        config_bump: [u8; 1],
    ) {
        self.state = AmmState::Initialized as u8;
        self.seed = seed;
        self.authority = authority;
        self.mint_x = mint_x;
        self.mint_y = mint_y;
        self.fee = fee;
        self.config_bump = config_bump;
    }

    #[inline(always)]
    pub unsafe fn set_state_unchecked(&mut self, state: u8) -> Result<(), ProgramError> {
        if state > AmmState::WithdrawOnly as u8 {
            return Err(ProgramError::InvalidAccountData);
        }

        self.state = state;

        Ok(())
    }

    pub unsafe fn set_fee_unchecked(&mut self, fee: u16) -> Result<(), ProgramError> {
        if fee > 10000 {
            return Err(ProgramError::InvalidAccountData);
        }

        self.fee = fee;

        Ok(())
    }

    pub unsafe fn set_authority_unchecked(
        &mut self,
        authority: Pubkey,
    ) -> Result<(), ProgramError> {
        self.authority = authority;

        Ok(())
    }

    #[inline(always)]
    pub fn has_authority(&self) -> Option<Pubkey> {
        let bytes = self.authority.as_ref();

        // SAFETY: [u8; 32] is always aligned and sized for 4 u64s
        let chunks: &[u64; 4] = unsafe { &*(bytes.as_ptr() as *const [u64; 4]) };
        if chunks.iter().any(|&x| x != 0) {
            // If any chunk is not 0, then hit an early exit and return the authority
            Some(self.authority)
        } else {
            // If all chunks are 0, then return None
            None
        }
    }
}
