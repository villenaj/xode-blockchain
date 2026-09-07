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
		#[extrinsic_call]
		_(RawOrigin::Root, who);
	}

	#[benchmark]
	fn thaw_account() {
		let who: T::AccountId = whitelisted_caller();
		AccountFreezer::<T>::freeze_account(RawOrigin::Root.into(), who.clone()).unwrap();
		#[extrinsic_call]
		_(RawOrigin::Root, who);
	}

	impl_benchmark_test_suite!(AccountFreezer, crate::mock::new_test_ext(), crate::mock::Test);
}
