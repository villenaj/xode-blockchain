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
	use sp_runtime::Saturating;
	use scale_info::prelude::vec::Vec;
	use scale_info::prelude::vec;
	use hex::decode;

	// Sessions
	use pallet_session::SessionManager;
	use sp_staking::SessionIndex;

	use frame_support::PalletId;
	use frame_support::traits::{Currency, ReservableCurrency};

	type BalanceOf<T> = <<T as Config>::StakingCurrency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

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

		/// The staking currency trait.
		type StakingCurrency: ReservableCurrency<Self::AccountId>;
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
		pub leaving: bool,
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

	/// Waiting candidates storage (Combination of the desired and proposed candidates)
	#[pallet::storage]
	pub type WaitingCandidates<T: Config> = StorageValue<_, BoundedVec<T::AccountId, T::MaxCandidates>, ValueQuery>;

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

		ProposedCandidateBonded  { _proposed_candidate: T::AccountId, },

		WaitingCandidateAdded { _waiting_candidate: T::AccountId, },

		ProposedCandidateLeft  { _proposed_candidate: T::AccountId, },
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
		ProposedCandidateNotFound,

		WaitingCandidateAlreadyExist,
		WaitingCandidateMaxExceeded,
		WaitingCandidatesEmpty,
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

		/// Register a new proposed candidate
		#[pallet::call_index(1)]
		#[pallet::weight(Weight::from_parts(10_000, 0) + T::DbWeight::get().writes(1))]
		pub fn register_proposed_candidate(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
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
				leaving: false,
            };
            ProposedCandidates::<T>::try_mutate(|candidates| -> Result<(), DispatchError> {
                candidates.try_push(candidate_info).map_err(|_| Error::<T>::ProposedCandidateMaxExceeded)?;
                Ok(())
            })?;
			Self::deposit_event(Event::ProposedCandidateAdded { _proposed_candidate: who });
			Ok(().into())
		}

		/// Bond proposed candidate
		/// Get the difference of the existing bond then effect the result: zero no change;
		/// if greater than zero, reserve the difference; otherwise unreserve.  Once the bond of 
		/// a candidate is updated, sort immediately the proposed candidates.
		/// Todo: How do we deal with the reserve and unreserve for some reason fails?
		#[pallet::call_index(2)]
		#[pallet::weight(Weight::from_parts(10_000, 0) + T::DbWeight::get().writes(1))]
		pub fn bond_proposed_candidate(origin: OriginFor<T>, new_bond: BalanceOf<T>,) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;
			let mut proposed_candidates = ProposedCandidates::<T>::get();
			let mut found = false;
			for i in 0..proposed_candidates.len() {
				if proposed_candidates[i].who == who {
					if proposed_candidates[i].bond > new_bond {
						let bond_diff = proposed_candidates[i].bond.saturating_sub(new_bond);
						proposed_candidates[i].bond = new_bond;
						if bond_diff > Zero::zero() {
							T::StakingCurrency::unreserve(&who, bond_diff);
						}
					} else {
						let bond_diff = new_bond.saturating_sub(proposed_candidates[i].bond);
						// Todo check if the bond_diff is zero, it is, check the waiting candidates, if not present
						// check the current session authors.  The candidate cannot set the bond to zero if it
						// still existing in the waiting candidates and current session authors
						proposed_candidates[i].bond = new_bond;
						if bond_diff > Zero::zero() {
							T::StakingCurrency::reserve(&who, bond_diff)?;
						}
					}
                    proposed_candidates[i].last_updated = frame_system::Pallet::<T>::block_number();
                    found = true;
                    break;
				}
			}
			ensure!(found, Error::<T>::ProposedCandidateNotFound);
			ProposedCandidates::<T>::put(proposed_candidates);
			Self::sort_proposed_candidates();
			Self::deposit_event(Event::ProposedCandidateBonded { _proposed_candidate: who });
			Ok(().into())
		}

		/// Leave Proposed Candidate
		#[pallet::call_index(3)]
		#[pallet::weight(Weight::from_parts(10_000, 0) + T::DbWeight::get().writes(1))]
		pub fn leave_proposed_candidate(origin: OriginFor<T>,) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;
			let mut proposed_candidates = ProposedCandidates::<T>::get();
			let mut found = false;
			for i in 0..proposed_candidates.len() {
				if proposed_candidates[i].who == who {
					proposed_candidates[i].leaving = true;
					proposed_candidates[i].last_updated = frame_system::Pallet::<T>::block_number();
					// Todo - update the waiting candidate
					found = true;
				}
			}
			ensure!(found, Error::<T>::ProposedCandidateNotFound);
			ProposedCandidates::<T>::put(proposed_candidates);
			Self::deposit_event(Event::ProposedCandidateLeft { _proposed_candidate: who });
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

		/// Add a waiting candidate
		/// At runtime instantiation the waiting candidates are the xaver nodes, prior sessions will now 
		/// include the proposed candidates. 
		pub fn add_waiting_candidate(waiting_candidate: T::AccountId) -> DispatchResult {
			WaitingCandidates::<T>::try_mutate(|waiting_candidates| -> DispatchResult {
				ensure!(!waiting_candidates.contains(&waiting_candidate), Error::<T>::WaitingCandidateAlreadyExist);
				waiting_candidates.try_push(waiting_candidate.clone()).map_err(|_| Error::<T>::WaitingCandidateMaxExceeded)?;
				Self::deposit_event(Event::WaitingCandidateAdded { _waiting_candidate: waiting_candidate });
				Ok(())
			})
		}

		/// Remove a waiting candidate
		/// Todo helper function
		/// pub fn remove_waiting_candidate(waiting_candidate: T::AccountId) -> DispatchResult 

		/// Add Xaver Nodes to the desired candidates at genesis
		pub fn add_xaver_nodes() {
			for xaver_node in T::XaverNodes::get() {
				let xaver_node = if xaver_node.starts_with("0x") { &xaver_node[2..] } else { xaver_node };
				// Panic if runtime not properly configured
				let decoded_bytes = decode(xaver_node).expect("Invalid hex string");
				// Panic if runtime not properly configured
				let authority = T::AuthorityId::decode(&mut decoded_bytes.as_slice()).expect("Error in decoding");
				let account = Self::authority_to_account(authority);
				let _ = Self::add_desired_candidate(account.clone());
				let _ = Self::add_waiting_candidate(account);
			}
		}

		/// Prepare waiting candidates
		/// This function is trigerred every session.  Waiting candidates is just a storage that
		/// merge the desired candidates and the proposed candidates.
		pub fn prepare_waiting_candidates() -> DispatchResult {
			let desired_candidates = DesiredCandidates::<T>::get();
			let proposed_candidates = ProposedCandidates::<T>::get();
			let mut waiting_candidates: BoundedVec<T::AccountId, T::MaxCandidates> = BoundedVec::default();

			// First, add all desired candidates
			for candidate in desired_candidates.iter() {
				if waiting_candidates.len() < T::MaxCandidates::get() as usize {
					waiting_candidates.try_push(candidate.clone()).map_err(|_| Error::<T>::WaitingCandidateAlreadyExist)?;
				}
			}

			// Next, add proposed candidates if there's still space
			for proposed_candidate in proposed_candidates.iter() {
				if waiting_candidates.len() < T::MaxCandidates::get() as usize {
					if !waiting_candidates.contains(&proposed_candidate.who) && 
					   !proposed_candidate.bond.is_zero() {
						waiting_candidates.try_push(proposed_candidate.who.clone()).map_err(|_| Error::<T>::WaitingCandidateAlreadyExist)?;
					}
				}
			}

			WaitingCandidates::<T>::put(waiting_candidates);

			Ok(())
		}

		/// Assemble the final collator nodes by updating the pallet_collator_selection invulnerables.
		/// This helper function is called every new session.
		/// Todo: If an existing invulnerable is not in the desired candidate list, the invulnerable
		///       must be delete from the current invulnerable.
		pub fn assemble_collators() -> DispatchResult {
			// Ensure the waiting candidates is not empty
			let waiting_candidates = WaitingCandidates::<T>::get();
			ensure!(!waiting_candidates.is_empty(), Error::<T>::WaitingCandidatesEmpty);

			// Remove the invulnerables not in the waiting candidates to save space
			let mut invulnerables = pallet_collator_selection::Invulnerables::<T>::get();
			invulnerables.retain(|account| waiting_candidates.contains(account));

			// Start inserting the waiting candidates to invulnerables
			for waiting_candidate in waiting_candidates.clone() {
				let _ = Self::add_invulnerable(waiting_candidate);
			}

			// Remove proposed candidates with zero bond


			// Prepare the new waiting candidates for the next session
			let _ = Self::prepare_waiting_candidates();

			Ok(())
		}

		/// Sort proposed candidates:
		/// Prioritize total_stake first, then bond, then oldest last_updated
		pub fn sort_proposed_candidates() {
            let mut proposed_candidates = ProposedCandidates::<T>::get();
            proposed_candidates.sort_by(|a, b| {
                b.total_stake.cmp(&a.total_stake)
                    .then_with(|| b.bond.cmp(&a.bond))
                    .then_with(|| a.last_updated.cmp(&b.last_updated)) 
            });
            ProposedCandidates::<T>::put(proposed_candidates);
        }
	}

	/// ===============
	/// Session Manager
	/// ===============
	impl<T: Config> SessionManager<T::AccountId> for Pallet<T> {
		fn new_session(_index: SessionIndex) -> Option<Vec<T::AccountId>> {
			let _ = Self::assemble_collators();
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
