use core::mem::size_of;
use pinocchio::{account_info::{AccountInfo, Ref, RefMut}, program_error::ProgramError, pubkey::Pubkey};

/// Some information about reading data
///  
/// - We use #[repr(C)] instead of #[repr(packed)] for safety
/// - Multi-byte integers stored as byte arrays to avoid alignment issues
/// - All field access is safe with no UB risk
#[repr(C)]
pub struct Config {
    state: u8,
    seed: [u8; 8],
    authority: Pubkey,
    mint_x: Pubkey,
    mint_y: Pubkey,
    fee: [u8; 2],
    config_bump: [u8; 1],
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

    /* Reading Helpers */

    // Return a `Config` from the given account info with safe borrowing.
    ///
    /// This method performs owner and length validation on `AccountInfo`.
    #[inline(always)]
    pub fn load(account_info: &AccountInfo) -> Result<Ref<Self>, ProgramError> {
        if account_info.data_len() != Self::LEN {
            return Err(ProgramError::InvalidAccountData);
        }
        if account_info.owner().ne(&crate::ID) {
            return Err(ProgramError::InvalidAccountData);
        }
        Ok(Ref::map(account_info.try_borrow_data()?, |data| unsafe {
            Self::from_bytes_unchecked(data)
        }))
    }

    /// Return a `Config` from the given account info.
    ///
    /// This method performs owner and length validation on `AccountInfo`, but does not
    /// perform the borrow check.
    ///
    /// # Safety
    ///
    /// The caller must ensure that it is safe to borrow the account data (e.g., there are
    /// no mutable borrows of the account data).
    #[inline(always)]
    pub unsafe fn load_unchecked(account_info: &AccountInfo) -> Result<&Self, ProgramError> {
        if account_info.data_len() != Self::LEN {
            return Err(ProgramError::InvalidAccountData);
        }
        if account_info.owner() != &crate::ID {
            return Err(ProgramError::InvalidAccountData);
        }
        Ok(Self::from_bytes_unchecked(
            account_info.borrow_data_unchecked(),
        ))
    }

    #[inline(always)]
    pub unsafe fn from_bytes_unchecked(bytes: &[u8]) -> &Self {
        &*(bytes.as_ptr() as *const Config)
    }

    #[inline(always)]
    pub fn state(&self) -> u8 {
        self.state
    }

    #[inline(always)]
    pub fn seed(&self) -> u64 {
        u64::from_le_bytes(self.seed)
    }

    #[inline(always)]
    pub fn authority(&self) -> &Pubkey {
        &self.authority
    }

    #[inline(always)]
    pub fn mint_x(&self) -> &Pubkey {
        &self.mint_x
    }

    #[inline(always)]
    pub fn mint_y(&self) -> &Pubkey {
        &self.mint_y
    }

    #[inline(always)]
    pub fn fee(&self) -> u16 {
        u16::from_le_bytes(self.fee)
    }

    #[inline(always)]
    pub fn config_bump(&self) -> [u8; 1] {
        self.config_bump
    }


    /* Writing Helpers */

    /// Return a mutable `Config` from the given account info with safe borrowing.
    ///
    /// This method performs owner and length validation on `AccountInfo`.
    #[inline(always)]
    pub fn load_mut(account_info: &AccountInfo) -> Result<RefMut<Self>, ProgramError> {
        if account_info.data_len() != Self::LEN {
            return Err(ProgramError::InvalidAccountData);
        }
        if account_info.owner().ne(&crate::ID) {
            return Err(ProgramError::InvalidAccountData);
        }
        Ok(RefMut::map(account_info.try_borrow_mut_data()?, |data| unsafe {
            Self::from_bytes_unchecked_mut(data)
        }))
    }

    /// Return a mutable `Config` from the given account info.
    ///
    /// This method performs owner and length validation on `AccountInfo`, but does not
    /// perform the borrow check.
    ///
    /// # Safety
    ///
    /// The caller must ensure that it is safe to borrow the account data mutably (e.g., there are
    /// no other borrows of the account data).
    #[inline(always)]
    pub unsafe fn load_mut_unchecked(account_info: &AccountInfo) -> Result<&mut Self, ProgramError> {
        if account_info.data_len() != Self::LEN {
            return Err(ProgramError::InvalidAccountData);
        }
        if account_info.owner() != &crate::ID {
            return Err(ProgramError::InvalidAccountData);
        }
        Ok(Self::from_bytes_unchecked_mut(
            account_info.borrow_mut_data_unchecked(),
        ))
    }

    /// Return a mutable `Config` from the given bytes.
    ///
    /// # Safety
    ///
    /// The caller must ensure that `bytes` contains a valid representation of `Config`, and
    /// it is properly aligned to be interpreted as an instance of `Config`.
    /// This method does not perform length validation.
    #[inline(always)]
    pub unsafe fn from_bytes_unchecked_mut(bytes: &mut [u8]) -> &mut Self {
        &mut *(bytes.as_mut_ptr() as *mut Config)
    }

    #[inline(always)]
    pub fn set_state(&mut self, state: u8) -> Result<(), ProgramError> {
        if state.ge(&(AmmState::WithdrawOnly as u8)) {
            return Err(ProgramError::InvalidAccountData);
        }

        self.state = state as u8;

        Ok(())
    }

    #[inline(always)]
    pub fn set_seed(&mut self, seed: u64) {
        self.seed = seed.to_le_bytes();
    }

    #[inline(always)]
    pub fn set_authority(&mut self, authority: Pubkey) {
        self.authority = authority;
    }

    #[inline(always)]
    pub fn set_mint_x(&mut self, mint_x: Pubkey) {
        self.mint_x = mint_x;
    }

    #[inline(always)]
    pub fn set_mint_y(&mut self, mint_y: Pubkey) {
        self.mint_y = mint_y;
    }

    #[inline(always)]
    pub fn set_fee(&mut self, fee: u16) -> Result<(), ProgramError> {
        if fee.ge(&10_000) {
            return Err(ProgramError::InvalidAccountData);
        }

        self.fee = fee.to_le_bytes();

        Ok(())
    }

    #[inline(always)]
    pub fn set_config_bump(&mut self, config_bump: [u8; 1]) {
        self.config_bump = config_bump;
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
    ) -> Result<(), ProgramError> {
        
        self.set_state(AmmState::Initialized as u8)?;
        self.set_seed(seed);
        self.set_authority(authority);
        self.set_mint_x(mint_x);
        self.set_mint_y(mint_y);
        self.set_fee(fee)?;
        self.set_config_bump(config_bump);

        Ok(())
    }

    #[inline(always)]
    pub fn has_authority(&self) -> Option<Pubkey> {
        let bytes = self.authority();

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
