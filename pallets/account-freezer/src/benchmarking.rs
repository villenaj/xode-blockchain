use super::*;
use crate::Pallet as AccountFreezer;
use frame_benchmarking::v2::*;
use frame_system::RawOrigin;

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn freeze_account() {
		let who: T::AccountId = whitelisted_caller();
		let duration = T::MaxFreezeDuration::get().unwrap_or_else(|| 1_000u32.into());
		#[extrinsic_call]
		_(RawOrigin::Root, who, duration);
	}

	#[benchmark]
	fn thaw_account() {
		let who: T::AccountId = whitelisted_caller();
		let duration = T::MaxFreezeDuration::get().unwrap_or_else(|| 1_000u32.into());
		AccountFreezer::<T>::freeze_account(RawOrigin::Root.into(), who.clone(), duration).unwrap();
		#[extrinsic_call]
		_(RawOrigin::Root, who);
	}

	#[benchmark]
	fn thaw_expired() {
		let who: T::AccountId = whitelisted_caller();
		AccountFreezer::<T>::freeze_account(RawOrigin::Root.into(), who.clone(), 0u32.into()).unwrap();
		#[extrinsic_call]
		_(RawOrigin::Signed(who.clone()), who);
	}

	impl_benchmark_test_suite!(AccountFreezer, crate::mock::new_test_ext(), crate::mock::Test);
}
