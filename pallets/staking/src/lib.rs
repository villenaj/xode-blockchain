//! # Xode Staking Pallet
//! 
//! XaverNodes
//! Candidates
//! DesiredCandidates
//! Invulnerables
//! 
//! Steps
//! 1. add_xaver_nodes (Block 0/Hooks)
//! 2. register_candidate
//! 3. bond_candidate
//! 
#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

pub mod weights;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

#[frame_support::pallet]
pub mod pallet {
	use frame_support::{dispatch::DispatchResultWithPostInfo, pallet_prelude::*, DefaultNoBound, };
	use frame_system::pallet_prelude::*;
	use sp_runtime::traits::{AccountIdConversion, BlockNumber, CheckedAdd, Zero, One};
	use scale_info::prelude::vec::Vec;
	use scale_info::prelude::vec;
	use hex::decode;

	// Sessions
	use pallet_session::SessionManager;
	use sp_staking::SessionIndex;

	use frame_support::PalletId;
	use frame_support::traits::{Currency, ReservableCurrency};

	type BalanceOf<T> = <<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

	/// Runtime configuration
	#[pallet::config]
	pub trait Config: pallet_balances::Config + pallet_collator_selection::Config + pallet_aura::Config + frame_system::Config {
		/// Because this pallet emits events, it depends on the runtime's definition of an event.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// A type representing the weights required by the dispatchables of this pallet.
		type WeightInfo: crate::weights::WeightInfo;

		/// The maximum proposed candidates
		type MaxProposedCandidates: Get<u32>;

		/// Xaver nodes that will be always present: vec!["",""]
		type XaverNodes: Get<&'static [&'static str]>;

		/// The currency trait.
		type Currency: ReservableCurrency<Self::AccountId>;
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);
	
	/// Candidate info
	#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug, scale_info::TypeInfo, MaxEncodedLen,)]
	pub struct CandidateInfo<AccountId, Balance, BlockNumber> {
		pub who: AccountId,
		pub bond: Balance,
		pub total_stake: Balance,
		pub last_updated: BlockNumber,
	}
		
	/// Proposed candidates 
	#[pallet::storage]
	pub type ProposedCandidates<T: Config> = StorageValue<
		_,
		BoundedVec<CandidateInfo<T::AccountId, BalanceOf<T>, BlockNumberFor<T>>, T::MaxCandidates>,
		ValueQuery,
	>;

	/// Desired candidates storage
	#[pallet::storage]
	pub type DesiredCandidates<T: Config> = StorageValue<_, BoundedVec<T::AccountId, T::MaxCandidates>, ValueQuery>;

	/// Next block number storage
	#[pallet::storage]
    #[pallet::getter(fn next_block_number)]
    pub type NextBlockNumber<T: Config> = StorageValue<_, BlockNumberFor<T>, OptionQuery>;


	/// ====================
	/// Events (past events)
	/// ====================
	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		InvulernableAdded { _invulnerable: T::AccountId, },

		DesiredCandidateAdded { _desired_candidate: T::AccountId, },

		TreasuryAccountRetrieved { _treasury: T::AccountId, _data: T::AccountData, },

		ProposedCandidateAdded  { _proposed_candidate: T::AccountId, },
	}

	/// ======
	/// Errors
	/// ======
	#[pallet::error]
	pub enum Error<T> {
		InvulnerableAlreadyExist,
		InvulnerableMaxExceeded,

		DesiredCandidateAlreadyExist,
		DesiredCandidateMaxExceeded,

		CandidateAlreadyExist,
		CandidateMaxExceeded,

		ProposedCandidateAlreadyExist,
		ProposedCandidateMaxExceeded,
	}

	/// =====
	/// Hooks
	/// =====
	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(_current_block: BlockNumberFor<T>) -> Weight {
			match NextBlockNumber::<T>::get() {
				Some(_next_block) => {
					// Todo, since this is replaced by session
					T::DbWeight::get().reads(1)
				}
				None => {
					// At block zero
					Self::add_xaver_nodes();
					T::DbWeight::get().reads(1)
				}
			}
		}
	}

	/// ===============
	/// Extrinsic Calls
	/// ===============
	#[pallet::call]
	impl<T: Config> Pallet<T> {

		/// Retrieve the treasury account
		#[pallet::call_index(0)]
		#[pallet::weight(Weight::from_parts(10_000, 0) + T::DbWeight::get().writes(1))]
		pub fn retrieve_treasury_account(_origin: OriginFor<T>) -> DispatchResultWithPostInfo {
			// May panic during runtime (Must fix!)
		 	let treasury= PalletId(*b"py/trsry").try_into_account().expect("Error converting to account");
		 	let account_info = frame_system::Pallet::<T>::account(&treasury);
		 	let account_data = account_info.data;	
		 	Self::deposit_event(Event::TreasuryAccountRetrieved { _treasury: treasury, _data: account_data, });
		 	Ok(().into())
		}

		/// Register a new candidate
		#[pallet::call_index(1)]
		#[pallet::weight(Weight::from_parts(10_000, 0) + T::DbWeight::get().writes(1))]
		pub fn register_candidate(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;
            ensure!(!ProposedCandidates::<T>::get().iter().any(|c| c.who == who), Error::<T>::ProposedCandidateAlreadyExist);
            ensure!(
                ProposedCandidates::<T>::get().len() < T::MaxProposedCandidates::get() as usize,
                Error::<T>::ProposedCandidateMaxExceeded
            );
            let candidate_info = CandidateInfo {
                who: who.clone(),
                bond: Zero::zero(),
                total_stake: Zero::zero(),
                last_updated: frame_system::Pallet::<T>::block_number(),
            };
            ProposedCandidates::<T>::try_mutate(|candidates| -> Result<(), DispatchError> {
                candidates.try_push(candidate_info).map_err(|_| Error::<T>::ProposedCandidateMaxExceeded)?;
                Ok(())
            })?;
			Self::deposit_event(Event::ProposedCandidateAdded { _proposed_candidate: who });
			Ok(().into())
		}

	}

	/// =======
	/// Helpers
	/// =======
	impl<T: Config> Pallet<T> {
		
		/// Convert AuthorityId to AccountId
		pub fn authority_to_account(authority: T::AuthorityId) -> T::AccountId {
			let authority_bytes = authority.encode();
			// Panic if not properly configured at runtime
			let account = <T as frame_system::Config>::AccountId::decode(&mut authority_bytes.as_slice()).unwrap();
			account
		}

		/// Add an account to pallet_collator_selection invulnerables
		/// https://github.com/paritytech/polkadot-sdk/blob/stable2409/cumulus/pallets/collator-selection/src/lib.rs#L841
		pub fn add_invulnerable (invulnerable: T::AccountId) -> DispatchResult {
			pallet_collator_selection::Invulnerables::<T>::try_mutate(|invulnerables| -> DispatchResult {
				ensure!(!invulnerables.contains(&invulnerable), Error::<T>::InvulnerableAlreadyExist);
				invulnerables.try_push(invulnerable.clone()).map_err(|_| Error::<T>::InvulnerableMaxExceeded)?;
				Self::deposit_event(Event::InvulernableAdded { _invulnerable: invulnerable });
				Ok(())
			})
		}	

		/// Add a desired candidate
		/// 1. Xaver nodes are automatically added to desired candidates (occupies the first priority)
		/// 2. The remaining slot will be coming from the candidates
		pub fn add_desired_candidate(desired_candidate: T::AccountId) -> DispatchResult {
			DesiredCandidates::<T>::try_mutate(|desired_candidates| -> DispatchResult {
				ensure!(!desired_candidates.contains(&desired_candidate), Error::<T>::DesiredCandidateAlreadyExist);
				desired_candidates.try_push(desired_candidate.clone()).map_err(|_| Error::<T>::DesiredCandidateMaxExceeded)?;
				Self::deposit_event(Event::DesiredCandidateAdded { _desired_candidate: desired_candidate });
				Ok(())
			})
		}

		/// Add Xaver Nodes to the desired candidates at genesis
		pub fn add_xaver_nodes() {
			for xaver_node in T::XaverNodes::get() {
				let xaver_node = if xaver_node.starts_with("0x") { &xaver_node[2..] } else { xaver_node };
				// Panic if runtime not properly configured
				let decoded_bytes = decode(xaver_node).expect("Invalid hex string");
				// Panic if runtime not properly configured
				let authority = T::AuthorityId::decode(&mut decoded_bytes.as_slice()).expect("Error in decoding");
				let _ = Self::add_desired_candidate(Self::authority_to_account(authority));
			}
		}

		/// Assemble the final collator nodes by updating the pallet_collator_selection invulnerables.
		/// This helper function is called every new session.
		/// Todo: If an existing invulnerable is not in the desired candidate list, the invulnerable
		///       must be delete from the current invulnerable.
		pub fn assemble_collators() {
			let desired_candidates = DesiredCandidates::<T>::get();
			for desired_candidate in desired_candidates.clone() {
				let _ = Self::add_invulnerable(desired_candidate);
			}
		}
	}

	/// ===============
	/// Session Manager
	/// ===============
	impl<T: Config> SessionManager<T::AccountId> for Pallet<T> {
		fn new_session(_index: SessionIndex) -> Option<Vec<T::AccountId>> {
			Self::assemble_collators();
			let  collators = pallet_collator_selection::Invulnerables::<T>::get().to_vec();
			Some(collators)
		}
		fn start_session(_: SessionIndex) {
			// Todo, may not do anything
		}
		fn end_session(_: SessionIndex) {
			// Todo, may not do anything
		}
	}

}
