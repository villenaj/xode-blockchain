//! # Xode Account Freezer Pallet
//!
//! This is free and unencumbered software released into the public domain.
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
	use frame_support::pallet_prelude::*;
	use frame_support::traits::fungible::{InspectFreeze, MutateFreeze};
	use frame_system::pallet_prelude::*;
	use sp_runtime::traits::Bounded;
	use sp_runtime::Saturating;

	pub type BalanceOf<T> = <<T as Config>::Currency as frame_support::traits::fungible::Inspect<
		<T as frame_system::Config>::AccountId,
	>>::Balance;

	#[pallet::config]
	pub trait Config: frame_system::Config {
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// The fungible implementation used to freeze/thaw the native (XON) balance.
		type Currency: MutateFreeze<Self::AccountId, Id = Self::RuntimeFreezeReason>
			+ InspectFreeze<Self::AccountId, Id = Self::RuntimeFreezeReason>;

		/// Overarching freeze reason; must be constructible from this pallet's own reason.
		type RuntimeFreezeReason: From<FreezeReason>;

		/// Origin allowed to freeze an account, and to force-thaw before expiry.
		type FreezeOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// Optional upper bound on the duration (in blocks) accepted by `freeze_account`.
		/// `None` means no maximum — any duration is accepted.
		#[pallet::constant]
		type MaxFreezeDuration: Get<Option<BlockNumberFor<Self>>>;

		type WeightInfo: WeightInfo;
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	/// Freeze reason contributed by this pallet, folded into `RuntimeFreezeReason`.
	#[pallet::composite_enum]
	pub enum FreezeReason {
		AccountFrozen,
	}

	/// Frozen accounts, mapped to the block at which they become eligible for thaw.
	#[pallet::storage]
	#[pallet::getter(fn frozen_until)]
	pub type FrozenUntil<T: Config> =
		StorageMap<_, Blake2_128Concat, T::AccountId, BlockNumberFor<T>, OptionQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// An account was frozen until the given block.
		AccountFrozen { who: T::AccountId, until: BlockNumberFor<T> },
		/// An account was thawed, either early by `FreezeOrigin` or after expiry.
		AccountThawed { who: T::AccountId },
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The account already has an active freeze.
		AlreadyFrozen,
		/// The account has no active freeze.
		NotFrozen,
		/// `thaw_expired` was called before the freeze's expiry block.
		NotYetExpired,
		/// Requested duration exceeds `MaxFreezeDuration`.
		DurationTooLong,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Freeze `who`'s entire transferable native balance for `duration` blocks.
		#[pallet::call_index(0)]
		#[pallet::weight(<T as Config>::WeightInfo::freeze_account())]
		pub fn freeze_account(
			origin: OriginFor<T>,
			who: T::AccountId,
			duration: BlockNumberFor<T>,
		) -> DispatchResult {
			T::FreezeOrigin::ensure_origin(origin)?;
			ensure!(!FrozenUntil::<T>::contains_key(&who), Error::<T>::AlreadyFrozen);
			if let Some(max) = T::MaxFreezeDuration::get() {
				ensure!(duration <= max, Error::<T>::DurationTooLong);
			}

			let until = frame_system::Pallet::<T>::block_number().saturating_add(duration);
			T::Currency::set_freeze(&FreezeReason::AccountFrozen.into(), &who, BalanceOf::<T>::max_value())?;
			FrozenUntil::<T>::insert(&who, until);

			Self::deposit_event(Event::AccountFrozen { who, until });
			Ok(())
		}

		/// Force-thaw an account before expiry. Restricted to `FreezeOrigin`.
		#[pallet::call_index(1)]
		#[pallet::weight(<T as Config>::WeightInfo::thaw_account())]
		pub fn thaw_account(origin: OriginFor<T>, who: T::AccountId) -> DispatchResult {
			T::FreezeOrigin::ensure_origin(origin)?;
			Self::do_thaw(who)
		}

		/// Permissionless cleanup: lift a freeze once its expiry block has passed.
		#[pallet::call_index(2)]
		#[pallet::weight(<T as Config>::WeightInfo::thaw_expired())]
		pub fn thaw_expired(origin: OriginFor<T>, who: T::AccountId) -> DispatchResult {
			ensure_signed(origin)?;
			let until = FrozenUntil::<T>::get(&who).ok_or(Error::<T>::NotFrozen)?;
			ensure!(frame_system::Pallet::<T>::block_number() >= until, Error::<T>::NotYetExpired);
			Self::do_thaw(who)
		}
	}

	impl<T: Config> Pallet<T> {
		fn do_thaw(who: T::AccountId) -> DispatchResult {
			ensure!(FrozenUntil::<T>::contains_key(&who), Error::<T>::NotFrozen);
			T::Currency::thaw(&FreezeReason::AccountFrozen.into(), &who)?;
			FrozenUntil::<T>::remove(&who);
			Self::deposit_event(Event::AccountThawed { who });
			Ok(())
		}
	}
}
