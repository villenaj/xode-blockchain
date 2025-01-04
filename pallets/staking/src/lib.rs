//! # Xode Staking Pallet
//! 
//! This is free and unencumbered software released into the public domain.
//!
//! Anyone is free to copy, modify, publish, use, compile, sell, or
//! distribute this software, either in source code form or as a compiled
//! binary, for any purpose, commercial or non-commercial, and by any
//! means.
//!
//! In jurisdictions that recognize copyright laws, the author or authors
//! of this software dedicate any and all copyright interest in the
//! software to the public domain. We make this dedication for the benefit
//! of the public at large and to the detriment of our heirs and
//! successors. We intend this dedication to be an overt act of
//! relinquishment in perpetuity of all present and future rights to this
//! software under copyright law.
//!
//! THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
//! EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
//! MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
//! IN NO EVENT SHALL THE AUTHORS BE LIABLE FOR ANY CLAIM, DAMAGES OR
//! OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE,
//! ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR
//! OTHER DEALINGS IN THE SOFTWARE.
//!
//! For more information, please refer to <http://unlicense.org>
#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

pub mod weights;
pub use weights::*;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::dispatch::DispatchResult;
	use frame_support::{dispatch::DispatchResultWithPostInfo, pallet_prelude::*, };
	use frame_system::pallet_prelude::*;
	use sp_runtime::traits::Zero;
	use sp_runtime::Saturating;
	use scale_info::prelude::vec::Vec;
	use scale_info::prelude::vec;
	use hex::decode;
	use frame_support::PalletId;

	// Sessions
	use pallet_session::SessionManager;
	use sp_staking::SessionIndex;

	use frame_support::traits::{Currency, ReservableCurrency};

	pub type BalanceOf<T> = <<T as Config>::StakingCurrency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

	/// Runtime configuration
	#[pallet::config]
	pub trait Config: pallet_balances::Config + 
		pallet_collator_selection::Config + 
		pallet_aura::Config + 
		pallet_authorship::Config + 
		pallet_session::Config + 
		frame_system::Config 
	{
		/// Because this pallet emits events, it depends on the runtime's definition of an event.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// A type representing the weights required by the dispatchables of this pallet.
		type WeightInfo: WeightInfo;

		/// The maximum proposed candidates
		type MaxProposedCandidates: Get<u32>;

		/// The maximum proposed candidate delegates
		type MaxProposedCandidateDelegates: Get<u32>;

		/// Xaver nodes that will be always present: vec!["",""]
		type XaverNodes: Get<&'static [&'static str]>;

		/// The staking currency trait.
		type StakingCurrency: ReservableCurrency<Self::AccountId>;

		/// The staking's pallet id, used for deriving its pot account ID.
		#[pallet::constant]
		type PalletId: Get<PalletId>;
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);
	
	#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug, scale_info::TypeInfo, MaxEncodedLen, PartialOrd)]
	pub enum Status {
		Offline = 0,
		Online = 1,
		Waiting = 2,
		Queuing = 3,
		Authoring = 4,
	}

	/// Candidate info
	#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug, scale_info::TypeInfo, MaxEncodedLen,)]
	pub struct CandidateInfo<AccountId, Balance, BlockNumber> {
		pub who: AccountId,
		pub bond: Balance,
		pub total_stake: Balance,
		pub last_updated: BlockNumber,
		pub leaving: bool,
		pub offline: bool,
		pub commission: u8,
		pub status: Status,
		pub status_level: u8,
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

	/// Delegator info (Delegation)
	#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug, scale_info::TypeInfo, MaxEncodedLen,)]
	pub struct Delegation<AccountId, Balance> {
		pub delegator: AccountId,
		pub stake: Balance,
	}

	/// Delegations
	#[pallet::storage]
	pub type Delegations<T: Config> = StorageMap<
		_, 
		Twox64Concat, 
		T::AccountId, 
		BoundedVec<Delegation<T::AccountId, BalanceOf<T>>, T::MaxProposedCandidateDelegates>, 
		OptionQuery
	>;

	/// Actual Authors are restarted every session
	#[pallet::storage]
	pub type ActualAuthors<T: Config> = StorageValue<_, BoundedVec<T::AccountId, T::MaxCandidates>, ValueQuery>;
		
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
		ProposedCandidateLeft  { _proposed_candidate: T::AccountId, },
		ProposedCandidateRemoved { _proposed_candidate: T::AccountId, },
		ProposedCandidateTotalStake { _proposed_candidate: T::AccountId, },
		ProposedCandidateCommissionSet { _proposed_candidate: T::AccountId, },
		ProposedCandidateOffline { _proposed_candidate: T::AccountId, },
		ProposedCandidateOnline { _proposed_candidate: T::AccountId, },

		WaitingCandidateAdded { _waiting_candidate: T::AccountId, },
		WaitingCandidateRemoved { _waiting_candidate: T::AccountId, },

		DelegationAdded { _delegator: T::AccountId, },
		DelegationRevoked { _delegator: T::AccountId, },
	}

	/// ======
	/// Errors
	/// ======
	#[pallet::error]
	pub enum Error<T> {
		InvulnerableAlreadyExist,
		InvulnerableMaxExceeded,
		InvulernableMember,

		DesiredCandidateAlreadyExist,
		DesiredCandidateMaxExceeded,

		CandidateAlreadyExist,
		CandidateMaxExceeded,

		ProposedCandidateAlreadyExist,
		ProposedCandidateMaxExceeded,
		ProposedCandidateNotFound,
		ProposedCandidateInvalidCommission,
		ProposedCandidateInsufficientBalance,
		ProposedCandidateNoSessionKeys,
		ProposedCandidateStillOnline,
		ProposedCandidateStillAuthoring,
		ProposedCandidateStillWaiting,
		ProposedCandidateStillQueuing,

		WaitingCandidateAlreadyExist,
		WaitingCandidateMaxExceeded,
		WaitingCandidateNotFound,
		WaitingCandidateMember,
		WaitingCandidatesEmpty,

		DelegationToSelfNotAllowed,
		DelegationInsufficientBalance,
		DelegationCandidateDoesNotExist,
		DelegationDelegatorDoesNotExist,
		DelegationsDoesNotExist,
		DelegationsMaxExceeded,

		ActualAuthorsAlreadyExist,
		ActualAuthorsMaxExceeded,

		AuraAuthorityMember,
	}

	/// =====
	/// Hooks
	/// =====
	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(_current_block: BlockNumberFor<T>) -> Weight {
			// Get the author
			if let Some(author) = pallet_authorship::Pallet::<T>::author() {
				let _ = Self::add_author(author.clone());
			}

			// Get the block number
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

		/// Register a new candidate in the Proposed Candidate list
		#[pallet::call_index(0)]
		#[pallet::weight(<weights::SubstrateWeight<T> as WeightInfo>::register_candidate())]
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
				leaving: false,
				offline: false,
				commission: 0,
				status: Status::Online,	
				status_level: 0,
            };
            ProposedCandidates::<T>::try_mutate(|candidates| -> Result<(), DispatchError> {
                candidates.try_push(candidate_info).map_err(|_| Error::<T>::ProposedCandidateMaxExceeded)?;
                Ok(())
            })?;
			Self::deposit_event(Event::ProposedCandidateAdded { _proposed_candidate: who });
			
			Ok(().into())
		}

		/// Bond Proposed Candidate
		/// Note:
		/// 	Get the difference of the existing bond then effect the result: zero no change;
		/// 	if greater than zero, reserve the difference; otherwise unreserve.  Once the bond of 
		/// 	a candidate is updated, sort immediately the proposed candidates.
		/// Todo: 
		/// 	How do we deal with the reserve and unreserve for some reason fails?
		#[pallet::call_index(1)]
		#[pallet::weight(<weights::SubstrateWeight<T> as WeightInfo>::bond_candidate())]
		pub fn bond_candidate(origin: OriginFor<T>, new_bond: BalanceOf<T>,) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;

			// Check if the candidate is registered.
			ensure!(ProposedCandidates::<T>::get().iter().any(|c| c.who == who), Error::<T>::ProposedCandidateNotFound); 

			// When we set the new bond to zero we assume that the candidate is leaving
			if new_bond == Zero::zero() {
				ensure!(!WaitingCandidates::<T>::get().contains(&who), Error::<T>::WaitingCandidateMember);
				ensure!(!pallet_collator_selection::Invulnerables::<T>::get().contains(&who), Error::<T>::InvulernableMember);
				ensure!(!Self::still_authoring(who.clone()), Error::<T>::AuraAuthorityMember);
			} else {
				ensure!(T::StakingCurrency::free_balance(&who) >= new_bond, Error::<T>::ProposedCandidateInsufficientBalance);
			}

			ProposedCandidates::<T>::mutate(|candidates| {
				if let Some(candidate) = candidates.iter_mut().find(|c| c.who == who) {
					if candidate.bond > new_bond {
						// Decrease the bond and reserve or might leave (new bond == 0)
						// Unreserve the difference, of the new bond is 0, unreserve the whole bond because the bond difference
						// is equal to the existing bond because any number subtracted by 0 would remain the same.
						let bond_diff = candidate.bond.saturating_sub(new_bond);
						candidate.bond = new_bond;
						if bond_diff > Zero::zero() {
							T::StakingCurrency::unreserve(&who, bond_diff);
						}
					} else {
						// Increase the bond and reserve.  If the new bond is greater than the existing bond then just replace
						// the current bond with the new one and get the difference to increase the reserve.
						let bond_diff = new_bond.saturating_sub(candidate.bond);
						candidate.bond = new_bond;
						if bond_diff > Zero::zero() {
							let _ = T::StakingCurrency::reserve(&who, bond_diff);
						}
					}
					candidate.last_updated = frame_system::Pallet::<T>::block_number();
				}
			});

			Self::sort_proposed_candidates();
			Self::deposit_event(Event::ProposedCandidateBonded { _proposed_candidate: who });
			Ok(().into())
		}

		/// Set commission
		/// Note:
		/// 	Numbers accepted are from 1 to 100 and no irrational numbers
		#[pallet::call_index(2)]
		#[pallet::weight(<weights::SubstrateWeight<T> as WeightInfo>::set_commission_of_candidate())]
		pub fn set_commission_of_candidate(origin: OriginFor<T>, commission: u8) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;
			// Commission control (1-100 percent only)
			ensure!(commission >= 1 && commission <= 100, Error::<T>::ProposedCandidateInvalidCommission);
			// Set commission
			ProposedCandidates::<T>::mutate(|candidates| {
				if let Some(candidate) = candidates.iter_mut().find(|c| c.who == who) {
					candidate.commission = commission;
					candidate.last_updated = frame_system::Pallet::<T>::block_number();
				}
			});

			Self::deposit_event(Event::ProposedCandidateCommissionSet { _proposed_candidate: who });
			Ok(().into())
		}

		/// Stake Proposed Candidate
		/// Note:
		/// 	To stake a proposed candidate means to delegate a balance for the candidate.
		/// 	The balance is reserved.
		/// 	The stake will remain in the storage even if the candidate leaves.
		/// Todo:
		/// 	How to handle the reservation if there is a failure in adding the delegation.
		/// 	Clean delegations when a candidate leaves to save space.
		#[pallet::call_index(3)]
		#[pallet::weight(<weights::SubstrateWeight<T> as WeightInfo>::stake_candidate())]
		pub fn stake_candidate(origin: OriginFor<T>, candidate: T::AccountId, amount: BalanceOf<T>) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;
			
			// Provide some controls
			ensure!(who != candidate, Error::<T>::DelegationToSelfNotAllowed);
			ensure!(ProposedCandidates::<T>::get().iter().any(|c| c.who == candidate), Error::<T>::DelegationCandidateDoesNotExist); 
			ensure!(T::StakingCurrency::free_balance(&who) >= amount, Error::<T>::DelegationInsufficientBalance);

			// Reserve the balance before updating the stake amount of the delegator
			let _ = T::StakingCurrency::reserve(&who, amount);

			// Update delegation stake amount
			let mut delegations = Delegations::<T>::get(&candidate).unwrap_or_default();
			if let Some(delegation) = delegations.iter_mut().find(|d| d.delegator == who) {
				delegation.stake += amount;
			} else {
				let _ = delegations.try_push(Delegation { delegator: who.clone(), stake: amount }).map_err(|_| Error::<T>::DelegationsMaxExceeded)?;
			}

			// Finally, update the storage
			Delegations::<T>::insert(&candidate, delegations);
			
			// Update the proposed candidate total stake amount
			let _ = Self::total_stake_proposed_candidate(candidate);
			Self::deposit_event(Event::DelegationAdded { _delegator: who });
			Ok(().into())
		}

		/// Un-stake Proposed Candidate
		/// Note:
		/// 	Remove first the delegation (stake amount) before un-reserving
		#[pallet::call_index(4)]
		#[pallet::weight(<weights::SubstrateWeight<T> as WeightInfo>::unstake_candidate())]
		pub fn unstake_candidate(origin: OriginFor<T>, candidate: T::AccountId) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;
			
			// Extract the delegations for that candidate
			let mut delegations = Delegations::<T>::get(&candidate).ok_or(Error::<T>::DelegationsDoesNotExist)?;
			
			// Extract the current stake of that delegator who wants to un-stake
			let position = delegations.iter().position(|c| c.delegator == who).ok_or(Error::<T>::DelegationDelegatorDoesNotExist)?;
			let stake_amount = delegations[position].stake;

			// New list of delegations without the delegator who un-stake
			delegations.retain(|c| c.delegator != who);

			// Update the delegation storage
			if delegations.is_empty() {
				// If there are no more delegators, remove the delegation for that candidate
				Delegations::<T>::remove(&candidate);
			} else {
				// Insert the new delegations without the delegator who un-stake
				Delegations::<T>::insert(&candidate, delegations);
			}

			// Finally, unreserve the balance
			T::StakingCurrency::unreserve(&who, stake_amount);

			// Update the proposed candidate total stake amount
			let _ = Self::total_stake_proposed_candidate(candidate);
			Self::deposit_event(Event::DelegationRevoked { _delegator: who });
			Ok(().into())
		}

		/// Offline Proposed Candidate 
		/// Note:
		///		Temporarily leave the candidacy without having to un-bond and un-stake.
		/// 	The offline status will be reflected only in the next session if the 
		/// 	candidate is already in the waiting list.
		#[pallet::call_index(5)]
		#[pallet::weight(<weights::SubstrateWeight<T> as WeightInfo>::offline_candidate())]
		pub fn offline_candidate(origin: OriginFor<T>,) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;
			let _ = Self::offline_proposed_candidate(who,true);
			Self::sort_proposed_candidates();
			Ok(().into())
		}

		/// Online Proposed Candidate 
		/// Note:
		///		Make the candidate online again.
		/// 	Todo: Check first the status if its already queuing
		#[pallet::call_index(6)]
		#[pallet::weight(<weights::SubstrateWeight<T> as WeightInfo>::online_candidate())]
		pub fn online_candidate(origin: OriginFor<T>,) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;
			let _ = Self::offline_proposed_candidate(who,false);
			Self::sort_proposed_candidates();
			Ok(().into())
		}

		/// Leave Proposed Candidate
		/// Note:
		/// 	Once the leaving flag is set to true, immediately remove the account in the
		/// 	Waiting Candidate list.
		#[pallet::call_index(7)]
		#[pallet::weight(<weights::SubstrateWeight<T> as WeightInfo>::leave_candidate())]
		pub fn leave_candidate(origin: OriginFor<T>,) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;

			let _ = ProposedCandidates::<T>::mutate(|candidates| {
				if let Some(candidate) = candidates.iter_mut().find(|c| c.who == who) {
					ensure!(candidate.offline, Error::<T>::ProposedCandidateStillOnline);
					ensure!(!WaitingCandidates::<T>::get().contains(&candidate.who), Error::<T>::ProposedCandidateStillWaiting);
					ensure!(!pallet_collator_selection::Invulnerables::<T>::get().contains(&candidate.who), Error::<T>::ProposedCandidateStillQueuing);
					ensure!(!Self::still_authoring(candidate.who.clone()), Error::<T>::ProposedCandidateStillAuthoring);
					candidate.leaving = true;
					candidate.last_updated = frame_system::Pallet::<T>::block_number();

					// Remove immediately from the waiting list.
					let _ = Self::remove_waiting_candidate(who.clone());
				}
				Ok::<(), Error<T>>(())
			});

			Self::deposit_event(Event::ProposedCandidateLeft { _proposed_candidate: who });
			Ok(().into())
		}

	}

	///	 =======
	/// Helpers
	/// =======
	impl<T: Config> Pallet<T> {
		
		/// Convert AuthorityId to AccountId
		/// Note:
		/// 	Needed since we use Hex values when we set the Xaver Nodes at runtime
		pub fn authority_to_account(authority: T::AuthorityId) -> T::AccountId {
			let authority_bytes = authority.encode();
			// Panic if not properly configured at runtime
			let account = <T as frame_system::Config>::AccountId::decode(&mut authority_bytes.as_slice()).unwrap();
			account
		}

		/// Add an account to pallet_collator_selection invulnerable
		/// Substrate Reference:
		/// 	https://github.com/paritytech/polkadot-sdk/blob/stable2409/cumulus/pallets/collator-selection/src/lib.rs#L841
		pub fn add_invulnerable (invulnerable: T::AccountId) -> DispatchResult {
			pallet_collator_selection::Invulnerables::<T>::try_mutate(|invulnerables| -> DispatchResult {
				ensure!(!invulnerables.contains(&invulnerable), Error::<T>::InvulnerableAlreadyExist);
				invulnerables.try_push(invulnerable.clone()).map_err(|_| Error::<T>::InvulnerableMaxExceeded)?;
				Self::deposit_event(Event::InvulernableAdded { _invulnerable: invulnerable });
				Ok(())
			})
		}	

		/// Add a desired candidate
		/// Note:
		/// 	Xaver nodes are automatically added to desired candidates (occupies the first priority).  
		/// 	The remaining slot will be coming from the proposed candidates.
		pub fn add_desired_candidate(desired_candidate: T::AccountId) -> DispatchResult {
			DesiredCandidates::<T>::try_mutate(|desired_candidates| -> DispatchResult {
				ensure!(!desired_candidates.contains(&desired_candidate), Error::<T>::DesiredCandidateAlreadyExist);
				desired_candidates.try_push(desired_candidate.clone()).map_err(|_| Error::<T>::DesiredCandidateMaxExceeded)?;
				Self::deposit_event(Event::DesiredCandidateAdded { _desired_candidate: desired_candidate });
				Ok(())
			})
		}

		/// Add Xaver Nodes to the desired candidates at genesis
		/// Note:
		/// 	The Xaver nodes (desired candidates must have a session keys)
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

		/// Remove a proposed candidate
		/// Note:
		/// 	This is called upon cleaning of the proposed candidate storage for a candidate who is leaving 
		/// 	regardless if it has still a bond.  Also called immediately after leaving.
		pub fn remove_proposed_candidate(proposed_candidate: T::AccountId) -> DispatchResult {
            ProposedCandidates::<T>::try_mutate(|proposed_candidates| -> DispatchResult {
                proposed_candidates.retain(|c| c.who != proposed_candidate); 
                Self::deposit_event(Event::ProposedCandidateRemoved { _proposed_candidate: proposed_candidate });
                Ok(())
            })
		}

		/// Change status of proposed candidate
		/// Note:
		/// 	1. Online - Upon registration and upon online
		/// 	2. Waiting - Set during a wait listing the author or downgrading through author preparation
		/// 	3. Queuing - Set during queuing and preparing the authors
		/// 	4. Authoring - Set during new_session, if the existing status is Authoring and the level
		///        is still zero, increment it by one.
		/// 	5. Offline - Manually set or during slashing
		/// 	6. Leaving is not a status, it is an event triggered by an extrinsic
		pub fn status_proposed_candidate(proposed_candidate: T::AccountId, status: Status) -> DispatchResult {
			ProposedCandidates::<T>::mutate(|candidates| {
				if let Some(candidate) = candidates.iter_mut().find(|c| c.who == proposed_candidate) {
					if status == Status::Authoring {
						if candidate.status == Status::Authoring && 
						   candidate.status_level == 0 {
							candidate.status_level = candidate.status_level + 1;
						}
					} 
					// Status authoring downgrading
					if candidate.status == Status::Authoring {
						if status < Status::Authoring {
							candidate.status_level = 0;
						}
					}

					candidate.status = status;
				}
			});
			Ok(().into())
		}

		/// Go offline/online proposed candidates
		/// Note:
		pub fn offline_proposed_candidate(proposed_candidate: T::AccountId, offline: bool) -> DispatchResult {
			ProposedCandidates::<T>::mutate(|candidates| {
				if let Some(candidate) = candidates.iter_mut().find(|c| c.who == proposed_candidate) {
					candidate.offline = offline;
					candidate.last_updated = frame_system::Pallet::<T>::block_number();
				}
			});
			if offline {
				Self::deposit_event(Event::ProposedCandidateOffline { _proposed_candidate: proposed_candidate });
			} else {
				Self::deposit_event(Event::ProposedCandidateOnline { _proposed_candidate: proposed_candidate });
			}
			Ok(().into())
		}

		/// Add a waiting candidate
		/// Note:
		/// 	At runtime instantiation the waiting candidates are the xaver nodes, prior sessions will now 
		/// 	include the sorted proposed candidates. 
		pub fn add_waiting_candidate(waiting_candidate: T::AccountId) -> DispatchResult {
			WaitingCandidates::<T>::try_mutate(|waiting_candidates| -> DispatchResult {
				ensure!(!waiting_candidates.contains(&waiting_candidate), Error::<T>::WaitingCandidateAlreadyExist);
				waiting_candidates.try_push(waiting_candidate.clone()).map_err(|_| Error::<T>::WaitingCandidateMaxExceeded)?;
				Self::deposit_event(Event::WaitingCandidateAdded { _waiting_candidate: waiting_candidate });
				Ok(())
			})
		}

		/// Remove a waiting candidate
		/// Note:	
		/// 	This is called when the candidate wants to leave.
		pub fn remove_waiting_candidate(waiting_candidate: T::AccountId) -> DispatchResult {
			WaitingCandidates::<T>::try_mutate(|waiting_candidates| -> DispatchResult {
				ensure!(waiting_candidates.contains(&waiting_candidate), Error::<T>::WaitingCandidateNotFound);
				if let Some(pos) = waiting_candidates.iter().position(|x| x == &waiting_candidate) {
					waiting_candidates.remove(pos);
				} else {
					return Err(Error::<T>::WaitingCandidateNotFound.into());
				}
				Self::deposit_event(Event::WaitingCandidateRemoved { _waiting_candidate: waiting_candidate });
				Ok(())
			})
		}

		/// Sort proposed candidates:
		/// Note:	
		/// 	Prioritize total_stake first, then bond, then oldest last_updated
		/// 	True = -1, False = 0
		pub fn sort_proposed_candidates() {
            let mut proposed_candidates = ProposedCandidates::<T>::get();
            proposed_candidates.sort_by(|a, b| {
                a.offline.cmp(&b.offline)
					.then_with(|| b.bond.cmp(&a.bond))
					.then_with(|| b.total_stake.cmp(&a.total_stake))
                    .then_with(|| a.last_updated.cmp(&b.last_updated)) 
            });
            ProposedCandidates::<T>::put(proposed_candidates);
        }

		/// Compute total_stake in the candidate information
		/// Note:
		/// 	Re-compute the total stake and called every staking extrinsic.
		/// 	Once the total is completed immediately sort the proposed candidates.
		pub fn total_stake_proposed_candidate(proposed_candidate: T::AccountId) -> DispatchResult {
			let mut proposed_candidates = ProposedCandidates::<T>::get();
			let mut found = false;
			for i in 0..proposed_candidates.len() {
				if proposed_candidates[i].who == proposed_candidate {
					let total_stake = if let Some(delegations) = <Delegations<T>>::get(proposed_candidate.clone()) {
						delegations.iter().fold(BalanceOf::<T>::default(), |acc, delegation| acc + delegation.stake)
					} else {
						BalanceOf::<T>::default()
					};
					proposed_candidates[i].total_stake = total_stake;
					proposed_candidates[i].last_updated = frame_system::Pallet::<T>::block_number();
					found = true;
					break;
				}
			}
			ensure!(found, Error::<T>::ProposedCandidateNotFound);
			ProposedCandidates::<T>::put(proposed_candidates);
			Self::sort_proposed_candidates();
			Self::deposit_event(Event::ProposedCandidateTotalStake { _proposed_candidate: proposed_candidate });			
			Ok(())
		}

		/// Add author
		/// Note:
		/// 	This helper function is called through hook on block initialization so as to include blocks with no
		/// 	transactions.
		pub fn add_author(author: T::AccountId) -> DispatchResult {
			ActualAuthors::<T>::try_mutate(|authors| {
				if !authors.contains(&author) {
					let _ = authors.try_push(author.clone());
				} 
				Ok(())
			})
		}

		/// Still authoring
		/// Note:
		/// 	This helper function is called in un-bonding (bond=zero) for leaving candidates.  The candidate
		/// 	must wait for the next session.
		pub fn still_authoring(who: T::AccountId) -> bool {
			let validators = pallet_session::Validators::<T>::get();
			let mut authoring = false;
			for validator in validators {
				let validator_bytes = validator.encode();
				let account = <T as frame_system::Config>::AccountId::decode(&mut validator_bytes.as_slice()).unwrap();
				if who == account {
					authoring = true;
					break;
				}
			}
			authoring
		}

		/// Slashed misbehaving authors
		/// Note:
		/// 	1. Desired candidates are exempted from slashing
		/// 	2. After slashing all the misbehaving authors, clean the actual authors for the 
		/// 	   next session.
		pub fn slashed_authors() -> DispatchResult {
			let validators = pallet_session::Validators::<T>::get();
			let authors = ActualAuthors::<T>::get();
			let desired_candidates = DesiredCandidates::<T>::get();

			let mut non_authors = Vec::new();
			for validator in validators {
				let validator_bytes = validator.encode();
				let account = <T as frame_system::Config>::AccountId::decode(&mut validator_bytes.as_slice()).unwrap();
				if !authors.contains(&account) && !desired_candidates.contains(&account) {
					non_authors.push(account.clone());
				}	
			}
			
			for _non_author in non_authors.iter() {
				// Todo: Slashed the author and make it offline, prerequisite Aura Round Robbin
				// let _ = Self::offline_proposed_candidate(non_author.clone(),true);
				// Self::sort_proposed_candidates();

				// Todo: Slashed the delegator for that author, Prerequisite Aura Round Robbin
			}
			
			// Clear the new set of actual authors
			ActualAuthors::<T>::put(BoundedVec::default());

			Ok(())
		}

		/// Wait-list the authors
		/// Note:
		/// 	1. Wait-list first the desired candidates.  Parameter to be wait listed.
		/// 		1.1. The bond is not zero
		/// 		1.2. Not leaving
		/// 		1.3. Not trying to go off-line
		/// 	2. Next, add the proposed candidates (must be already prepared and sorted)
		/// 	3. If the status of the proposed candidate is still Online, change it to waiting, otherwise
		/// 	   retain.
		pub fn wait_list_authors() -> DispatchResult {
			let desired_candidates = DesiredCandidates::<T>::get();
			let proposed_candidates = ProposedCandidates::<T>::get();
			let mut waiting_candidates: BoundedVec<T::AccountId, T::MaxCandidates> = BoundedVec::default();

			// First, add all desired candidates
			for candidate in desired_candidates.iter() {
				if waiting_candidates.len() < T::MaxCandidates::get() as usize {
					waiting_candidates.try_push(candidate.clone()).map_err(|_| Error::<T>::WaitingCandidateAlreadyExist)?;
				}
			}
			// Next, add proposed candidates if there's still space, bond is not zero and not leaving
			for proposed_candidate in proposed_candidates.iter() {
				if waiting_candidates.len() < T::MaxCandidates::get() as usize {
					if !waiting_candidates.contains(&proposed_candidate.who) && 
					   !proposed_candidate.bond.is_zero() && 
					   !proposed_candidate.leaving && 
					   !proposed_candidate.offline {

						waiting_candidates.try_push(proposed_candidate.who.clone()).map_err(|_| Error::<T>::WaitingCandidateAlreadyExist)?;

						if proposed_candidate.status == Status::Online {
							let _ = Self::status_proposed_candidate(proposed_candidate.who.clone(),Status::Waiting);
						}
					}
				}
			}
			WaitingCandidates::<T>::put(waiting_candidates);
			Ok(())
		}

		/// Prepare the authors to be wait listed.
		/// Note:
		/// 	1. We need to execute this helper function to make sure that we have space for others to join.
		/// 	2. Remove only the candidate if it is leaving or the bond is zero.
		/// 	3. If the candidate status is Online or Offline, remove it immediately from the proposed candidate
		/// 	4. If the candidate status is Waiting, remove it from the waiting candidate and immediately remove
		/// 	   from the proposed candidate.
		/// 	5. For Queuing candidate, just change the status to waiting.
		/// 	6. For Authoring candidate, just change the status to queuing.
		pub fn prepare_authors() -> DispatchResult {
			let proposed_candidates = ProposedCandidates::<T>::get();

			for proposed_candidate in proposed_candidates.iter() {
				if proposed_candidate.offline || 
				   proposed_candidate.leaving || 
				   proposed_candidate.bond == Zero::zero() {
					if proposed_candidate.status == Status::Online || 
					   proposed_candidate.status == Status::Offline {
						// Todo: un-stake the delegates before removing the proposed candidates

						// Remove from the proposed candidates
						let _ = Self::remove_proposed_candidate(proposed_candidate.who.clone());
					} else if proposed_candidate.status == Status::Waiting {
						// Todo: un-stake the delegates before removing the proposed candidates

						// Remove immediately from the waiting and proposed candidates
						let _ = Self::remove_waiting_candidate(proposed_candidate.who.clone());
						let _ = Self::remove_proposed_candidate(proposed_candidate.who.clone());
					} else if proposed_candidate.status == Status::Queuing {
						// Change status to Waiting (Downgrading the status)
						let _ = Self::status_proposed_candidate(proposed_candidate.who.clone(),Status::Waiting);
					} else {
						// Change status to Queuing (Downgrading the status)
						let _ = Self::status_proposed_candidate(proposed_candidate.who.clone(),Status::Queuing);
					}
				}
			}

			Ok(())
		}

		/// Queue authors by updating the pallet_collator_selection invulnerable.
		/// Note:
		/// 	1. This helper function is called at the end of each sessions.
		/// 	2. Queue the authors using the waiting candidates.
		/// 	3. Change the status of the newly added candidates from waiting
		/// 	   to queuing.
		pub fn queue_authors() -> DispatchResult {
			// Ensure the waiting candidates storage is not empty
			let waiting_candidates = WaitingCandidates::<T>::get();
			ensure!(!waiting_candidates.is_empty(), Error::<T>::WaitingCandidatesEmpty);

			// Remove the invulnerable not in the waiting candidates to save space
			let mut invulnerables = pallet_collator_selection::Invulnerables::<T>::get();
			invulnerables.retain(|account| waiting_candidates.contains(account));
			pallet_collator_selection::Invulnerables::<T>::put(invulnerables);

			// Start inserting the waiting candidates to invulnerables
			for waiting_candidate in waiting_candidates.clone() {
				let _ = Self::add_invulnerable(waiting_candidate.clone());

				// Change status to Queuing if the status is waiting, otherwise
				// retain current status.
				ProposedCandidates::<T>::mutate(|candidates| {
					if let Some(candidate) = candidates.iter_mut().find(|c| c.who == waiting_candidate) {
						if candidate.status == Status::Waiting {
							let _ = candidate.status == Status::Queuing;
						}
					}
				});
			}

			Ok(())
		}
	}

	/// ===============
	/// Session Manager
	/// ===============
	impl<T: Config> SessionManager<T::AccountId> for Pallet<T> {
		fn new_session(_index: SessionIndex) -> Option<Vec<T::AccountId>> {
			let  authors = pallet_collator_selection::Invulnerables::<T>::get().to_vec();
			for author in authors.clone() {
				// Change status to Authoring (The top status)
				let _ = Self::status_proposed_candidate(author.clone(),Status::Authoring);
			}			
			// Set the authors
			Some(authors)
		}
		
		fn start_session(_: SessionIndex) {
			// Todo or may not do anything
		}

		fn end_session(_: SessionIndex) {
			// Queue the authors from the waiting list
			let _ = Self::queue_authors();

			// Slashed the authors of the current session
			let _ = Self::slashed_authors();

			// Prepare the authors for the next waiting list
			let _ = Self::prepare_authors();

			// Wait list the authors
			let _ = Self::wait_list_authors();
		}
	}

	/// ===============
	/// Authorship
	/// ===============
	impl<T: Config> pallet_authorship::EventHandler<T::AccountId, BlockNumberFor<T>> for Pallet<T> {
		fn note_author(_author: T::AccountId) {
			// TODO: transfer fees here
		}

	}

}
