use crate::mock::*;
use crate::Error;
use frame_support::{assert_noop, assert_ok};
use frame_system::RawOrigin;

#[test]
fn freeze_blocks_transfer_and_thaw_lifts_it() {
	new_test_ext().execute_with(|| {
		assert_ok!(AccountFreezer::freeze_account(RawOrigin::Root.into(), 1));

		// frozen: transfer should fail
		assert_noop!(
			<Balances as frame_support::traits::fungible::Mutate<AccountId>>::transfer(
				&1, &2, 100, frame_support::traits::tokens::Preservation::Preserve
			),
			sp_runtime::TokenError::Frozen
		);

		assert_ok!(AccountFreezer::thaw_account(RawOrigin::Root.into(), 1));

		assert_ok!(<Balances as frame_support::traits::fungible::Mutate<AccountId>>::transfer(
			&1, &2, 100, frame_support::traits::tokens::Preservation::Preserve
		));
	});
}

#[test]
fn cannot_double_freeze() {
	new_test_ext().execute_with(|| {
		assert_ok!(AccountFreezer::freeze_account(RawOrigin::Root.into(), 1));
		assert_noop!(
			AccountFreezer::freeze_account(RawOrigin::Root.into(), 1),
			Error::<Test>::AlreadyFrozen
		);
	});
}

#[test]
fn non_root_cannot_freeze() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			AccountFreezer::freeze_account(RawOrigin::Signed(2).into(), 1),
			frame_support::error::BadOrigin
		);
	});
}

#[test]
fn force_origin_can_thaw() {
	new_test_ext().execute_with(|| {
		assert_ok!(AccountFreezer::freeze_account(RawOrigin::Root.into(), 1));
		assert_ok!(AccountFreezer::thaw_account(RawOrigin::Root.into(), 1));
		assert_noop!(
			AccountFreezer::thaw_account(RawOrigin::Root.into(), 1),
			Error::<Test>::NotFrozen
		);
	});
}
