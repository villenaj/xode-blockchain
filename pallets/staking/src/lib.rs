//! # Xode Staking Pallet
//!
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

// <https://paritytech.github.io/polkadot-sdk/master/polkadot_sdk_docs/polkadot_sdk/frame_runtime/index.html>
// <https://paritytech.github.io/polkadot-sdk/master/polkadot_sdk_docs/guides/your_first_pallet/index.html>
//
// To see a full list of `pallet` macros and their use cases, see:
// <https://paritytech.github.io/polkadot-sdk/master/pallet_example_kitchensink/index.html>
// <https://paritytech.github.io/polkadot-sdk/master/frame_support/pallet_macros/index.html>
#[frame_support::pallet]
pub mod pallet {
	use frame_support::{dispatch::DispatchResultWithPostInfo, pallet_prelude::*, DefaultNoBound, };
	use frame_system::pallet_prelude::*;
	use sp_runtime::traits::{CheckedAdd, One,};
	use scale_info::prelude::vec::Vec;
	use scale_info::prelude::vec;
	use hex::decode;

	// Sessions
	use pallet_session::SessionManager;
	use sp_staking::SessionIndex;

	/// Configure the pallet by specifying the parameters and types on which it depends.
	#[pallet::config]
	pub trait Config: pallet_collator_selection::Config + pallet_aura::Config + frame_system::Config {
		/// Because this pallet emits events, it depends on the runtime's definition of an event.
		/// <https://paritytech.github.io/polkadot-sdk/master/polkadot_sdk_docs/reference_docs/frame_runtime_types/index.html>
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// A type representing the weights required by the dispatchables of this pallet.
		type WeightInfo: crate::weights::WeightInfo;

		/// Maximum Candidates (Must match with Aura's maximum authorities)
		// type MaxCandidates: Get<u32>;

		/// Block interval (used to determine the next block number)
		type BlockInterval: Get<u32>;

		/// Authorities that will be always present: vec!["",""]
		type Invulnerables: Get<&'static [&'static str]>;
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	/// A struct to store a single block-number. Has all the right derives to store it in storage.
	/// <https://paritytech.github.io/polkadot-sdk/master/polkadot_sdk_docs/reference_docs/frame_storage_derives/index.html>
	#[derive(
		Encode, Decode, MaxEncodedLen, TypeInfo, CloneNoBound, PartialEqNoBound, DefaultNoBound,
	)]
	#[scale_info(skip_type_params(T))]
	pub struct CompositeStruct<T: Config> {
		/// A block number.
		pub(crate) block_number: BlockNumberFor<T>,
	}

	/// Candidates
	#[pallet::storage]
	pub type Candidates<T: Config> = StorageValue<_, BoundedVec<T::AuthorityId, T::MaxCandidates>, ValueQuery>;

	/// Next block number
	#[pallet::storage]
    #[pallet::getter(fn next_block_number)]
    pub type NextBlockNumber<T: Config> = StorageValue<_, BlockNumberFor<T>, OptionQuery>;

	/// The pallet's storage items.
	/// <https://paritytech.github.io/polkadot-sdk/master/polkadot_sdk_docs/guides/your_first_pallet/index.html#storage>
	/// <https://paritytech.github.io/polkadot-sdk/master/frame_support/pallet_macros/attr.storage.html>
	#[pallet::storage]
	pub type Something<T: Config> = StorageValue<_, CompositeStruct<T>>;

	/// Pallets use events to inform users when important changes are made.
	/// <https://paritytech.github.io/polkadot-sdk/master/polkadot_sdk_docs/guides/your_first_pallet/index.html#event-and-error>
	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// We usually use passive tense for events.
		SomethingStored { block_number: BlockNumberFor<T>, who: T::AccountId },
		AuthoritiesRetrieved { authorities: Vec<T::AuthorityId>,},
		ValidatorsRetrieved { validators: Vec<T::AccountId>,},
		MaxAuthoritiesRetrieved { max_authorities: u32,},
		CandidateAdded { candidate: T::AuthorityId, },
		CandidateRemoved { candidate: T::AuthorityId, },
		AuthorityAdded { authority: T::AuthorityId, },
		AuthorityRemoved { authority: T::AuthorityId, },
		CollatorAdded { collator: T::AccountId, },
	}

	/// Errors inform users that something went wrong.
	/// <https://paritytech.github.io/polkadot-sdk/master/polkadot_sdk_docs/guides/your_first_pallet/index.html#event-and-error>
	#[pallet::error]
	pub enum Error<T> {
		/// Error names should be descriptive.
		NoneValue,
		/// Errors should have helpful documentation associated with them.
		StorageOverflow,

		/// Staking errors
		ExceedsMaxCandidates,
		ErrorAddingCandidate,
		CandidateAlreadyExist,
		
		ErrorAddingAuthority,
		ErrorRemovingAuthority,
		AuthorityAlreadyExist,
		AuthorityDoesNotExist,
		AuthorityDoesNotExistInCandidates,
		ExceedsMaxAuthorities,

		CollatorAlreadyExist,
		CollatorDoesNotExist,
		CollatorDoesNotExistInCandidates,
		ExceedsMaxCollators,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(current_block: BlockNumberFor<T>) -> Weight {
			match NextBlockNumber::<T>::get() {
				Some(next_block) => {
					if current_block == next_block {
						//Self::merge_candidates();

						Self::update_next_block_number(current_block);
					}
					T::DbWeight::get().reads(1)
				}
				None => {
					Self::add_invulnerables();

					Self::update_next_block_number(current_block); 
					
					T::DbWeight::get().reads(1)
				}
			}
		}
	}

	/// Calls
	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// An example dispatchable that takes a singles value as a parameter, writes the value to
		/// storage and emits an event. This function must be dispatched by a signed extrinsic.
		#[pallet::call_index(0)]
		#[pallet::weight(Weight::from_parts(10_000, 0) + T::DbWeight::get().writes(1))]
		pub fn do_something(origin: OriginFor<T>, bn: u32) -> DispatchResultWithPostInfo {
			// Check that the extrinsic was signed and get the signer.
			// This function will return an error if the extrinsic is not signed.
			// <https://paritytech.github.io/polkadot-sdk/master/polkadot_sdk_docs/reference_docs/frame_origin/index.html>
			let who = ensure_signed(origin)?;

			// Convert the u32 into a block number. This is possible because the set of trait bounds
			// defined in [`frame_system::Config::BlockNumber`].
			let block_number: BlockNumberFor<T> = bn.into();

			// Update storage.
			<Something<T>>::put(CompositeStruct { block_number });

			// Emit an event.
			Self::deposit_event(Event::SomethingStored { block_number, who });

			// Return a successful [`DispatchResultWithPostInfo`] or [`DispatchResult`].
			Ok(().into())
		}

		/// An example dispatchable that may throw a custom error.
		#[pallet::call_index(1)]
		#[pallet::weight(Weight::from_parts(10_000, 0) + T::DbWeight::get().reads_writes(1,1))]
		pub fn cause_error(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
			let _who = ensure_signed(origin)?;

			// Read a value from storage.
			match <Something<T>>::get() {
				// Return an error if the value has not been set.
				None => Err(Error::<T>::NoneValue)?,
				Some(mut old) => {
					// Increment the value read from storage; will error in the event of overflow.
					old.block_number = old
						.block_number
						.checked_add(&One::one())
						// ^^ equivalent is to:
						// .checked_add(&1u32.into())
						// both of which build a `One` instance for the type `BlockNumber`.
						.ok_or(Error::<T>::StorageOverflow)?;
					// Update the value in storage with the incremented result.
					<Something<T>>::put(old);
					// Explore how you can rewrite this using
					// [`frame_support::storage::StorageValue::mutate`].
					Ok(().into())
				},
			}
		}

		/// Retrieve the authorities in the Aura pallet.
		#[pallet::call_index(2)]
		#[pallet::weight(Weight::from_parts(10_000, 0) + T::DbWeight::get().writes(1))]
		pub fn retrieve_authorities(_origin: OriginFor<T>) -> DispatchResultWithPostInfo {
			let authorities = pallet_aura::Authorities::<T>::get();
			Self::deposit_event(Event::AuthoritiesRetrieved { authorities: authorities.iter().cloned().collect() });
			Ok(().into())
		}

		#[pallet::call_index(3)]
		#[pallet::weight(Weight::from_parts(10_000, 0) + T::DbWeight::get().writes(1))]
		pub fn retrieve_max_authorities(_origin: OriginFor<T>) -> DispatchResultWithPostInfo {
			// Todo: Convert this code by getting the maximum authorities not the number of authorities
			Self::deposit_event(Event::MaxAuthoritiesRetrieved { max_authorities: pallet_aura::Authorities::<T>::decode_len().unwrap_or(0) as u32});
			Ok(().into())
		}

		#[pallet::call_index(4)]
		#[pallet::weight(Weight::from_parts(10_000, 0) + T::DbWeight::get().writes(1))]
		pub fn stake(_origin: OriginFor<T>, new_candidate: T::AuthorityId) -> DispatchResult {
			let _who = ensure_signed(_origin)?;
        	Candidates::<T>::try_mutate(|candidates| -> DispatchResult {
				ensure!(!candidates.contains(&new_candidate), "Candidate already exists");
				candidates.try_push(new_candidate.clone()).map_err(|_| "Max candidates reached")?;

				Self::deposit_event(Event::CandidateAdded { candidate: new_candidate });
				
				Ok(())
			})
		}

		#[pallet::call_index(5)] // Increment the call index appropriately
		#[pallet::weight(Weight::from_parts(10_000, 0) + T::DbWeight::get().writes(1))]
		pub fn unstake(_origin: OriginFor<T>, candidate_to_remove: T::AuthorityId) -> DispatchResult {
			let _who = ensure_signed(_origin)?;
			Candidates::<T>::try_mutate(|candidates| -> DispatchResult {
				ensure!(candidates.contains(&candidate_to_remove), "Candidate does not exist");
				if let Some(pos) = candidates.iter().position(|x| x == &candidate_to_remove) {
					candidates.remove(pos);
				} else {
					return Err("Failed to remove candidate".into());
				}
		
				Self::deposit_event(Event::CandidateRemoved { candidate: candidate_to_remove });
				
				Ok(())
			})
		}

		/// Retrieve the authorities in the Aura pallet.
		#[pallet::call_index(6)]
		#[pallet::weight(Weight::from_parts(10_000, 0) + T::DbWeight::get().writes(1))]
		pub fn retrieve_validators(_origin: OriginFor<T>) -> DispatchResultWithPostInfo {
			let mut validators: Vec<T::AccountId> = vec![];
			for authority in pallet_aura::Authorities::<T>::get(){
				let account = Self::convert_to_account(authority);
				validators.push(account);
			}
			Self::deposit_event(Event::ValidatorsRetrieved { validators: validators });
			Ok(().into())
		}
	}

	/// Helper functions
	impl<T: Config> Pallet<T> {
		
		/// Convert AuthorityId to AccountId
		pub fn convert_to_account(authority: T::AuthorityId) -> T::AccountId {
			let authority_bytes = authority.encode();
			let account = <T as frame_system::Config>::AccountId::decode(&mut authority_bytes.as_slice()).unwrap();
			account
		}

		/// Add a candidate
		pub fn add_candidate(candidate: T::AuthorityId) -> DispatchResult {
			Candidates::<T>::try_mutate(|candidates| -> DispatchResult {
				// Search if the candidate already exist
				ensure!(!candidates.contains(&candidate), Error::<T>::CandidateAlreadyExist);
				// Push the candidate
				candidates.try_push(candidate.clone()).map_err(|_| Error::<T>::ExceedsMaxCandidates)?;
				// Log the event
				Self::deposit_event(Event::CandidateAdded { candidate: candidate });
				Ok(())
			})
		}

		/// Add an authority
		pub fn add_authority(authority: T::AuthorityId) -> DispatchResult {
			pallet_aura::Authorities::<T>::try_mutate(|authorities| -> DispatchResult {
				// Search if the authority already exist
				ensure!(!authorities.contains(&authority), Error::<T>::AuthorityAlreadyExist);
				// Push the authority
				authorities.try_push(authority.clone()).map_err(|_| Error::<T>::ExceedsMaxAuthorities)?;
				// Log the event
				Self::deposit_event(Event::AuthorityAdded { authority: authority });
				Ok(())
			})
		}

		/// Delete an authority
		pub fn delete_authority(authority: T::AuthorityId) -> DispatchResult {
			pallet_aura::Authorities::<T>::try_mutate(|authorities| -> DispatchResult {
				// Search the authority
				ensure!(authorities.contains(&authority), Error::<T>::AuthorityDoesNotExist);
				// Remove the authority
				if let Some(pos) = authorities.iter().position(|x| x == &authority) {
					authorities.remove(pos);
				} else {
					return Err(Error::<T>::ErrorRemovingAuthority.into());
				}
				// Log event
				Self::deposit_event(Event::AuthorityRemoved { authority: authority });
				Ok(())
			})
		}

		/// Add a collator
		pub fn add_collator(collator: T::AccountId) -> DispatchResult {
			// https://github.com/paritytech/polkadot-sdk/blob/stable2409/cumulus/pallets/collator-selection/src/lib.rs#L841
			pallet_collator_selection::Invulnerables::<T>::try_mutate(|collators| -> DispatchResult {
				// Search if the invulnerable already exist
				ensure!(!collators.contains(&collator), Error::<T>::CollatorAlreadyExist);
				// Push the authority
				collators.try_push(collator.clone()).map_err(|_| Error::<T>::ExceedsMaxCollators)?;
				// Log the event
				Self::deposit_event(Event::CollatorAdded { collator: collator });
				Ok(())
			})
		}

		/// Update the next block number event trigger
		pub fn update_next_block_number(current_block: BlockNumberFor<T>) {
			let interval = T::BlockInterval::get();
			let new_block = current_block + BlockNumberFor::<T>::from(interval);
			NextBlockNumber::<T>::put(new_block);
		}	

		/// Push the invulnerables define in the Runtime
		pub fn add_invulnerables() {
			for invulnerable in T::Invulnerables::get() {
				let invulnerable = if invulnerable.starts_with("0x") { &invulnerable[2..] } else { invulnerable };
				let decoded_bytes = decode(invulnerable).expect("Invalid hex string");
				let candidate = T::AuthorityId::decode(&mut decoded_bytes.as_slice()).expect("Error in decoding");
				let _ = Self::add_candidate(candidate);
			}
		}

		/// Merge staking candidates, aura authorities and collator invulnerables
		pub fn merge_candidates() {
			let candidates = Candidates::<T>::get();
			for candidate in candidates.clone() {
				// Add candidates to athorities
				// let _ = Self::add_authority(candidate.clone());
				// Add candidates to collators
				let account = Self::convert_to_account(candidate);
				let _ = Self::add_collator(account);
			}
			// Delete authorities if not found in candidates
			//for authority in pallet_aura::Authorities::<T>::get() {
			//	if !candidates.contains(&authority) {
			//		let _ = Self::delete_authority(authority);
			//	} 	
			//}
		}

	}

	/// Session Manager
	impl<T: Config> SessionManager<T::AccountId> for Pallet<T> {
		fn new_session(index: SessionIndex) -> Option<Vec<T::AccountId>> {
			Self::merge_candidates();
			let  collators = pallet_collator_selection::Invulnerables::<T>::get().to_vec();
			Some(collators)
		}
		fn start_session(_: SessionIndex) {
			// todo
		}
		fn end_session(_: SessionIndex) {
			// todo
		}
	}

}
