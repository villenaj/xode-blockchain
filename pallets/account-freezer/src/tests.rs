use crate::mock::*;
use crate::Error;
use frame_support::{assert_noop, assert_ok, traits::fungible::Mutate};
use frame_system::RawOrigin;

#[test]
fn freeze_blocks_transfer_and_thaw_expired_lifts_it() {
	new_test_ext().execute_with(|| {
		System::set_block_number(1);
		assert_ok!(AccountFreezer::freeze_account(RawOrigin::Root.into(), 1, 10));

		// frozen: transfer should fail
		assert_noop!(
			<Balances as frame_support::traits::fungible::Mutate<AccountId>>::transfer(
				&1, &2, 100, frame_support::traits::tokens::Preservation::Preserve
			),
			sp_runtime::TokenError::Frozen
		);

		// too early to self-thaw
		assert_noop!(
			AccountFreezer::thaw_expired(RawOrigin::Signed(2).into(), 1),
			Error::<Test>::NotYetExpired
		);

		System::set_block_number(11);
		assert_ok!(AccountFreezer::thaw_expired(RawOrigin::Signed(2).into(), 1));

		assert_ok!(<Balances as frame_support::traits::fungible::Mutate<AccountId>>::transfer(
			&1, &2, 100, frame_support::traits::tokens::Preservation::Preserve
		));
	});
}

#[test]
fn cannot_double_freeze() {
	new_test_ext().execute_with(|| {
		assert_ok!(AccountFreezer::freeze_account(RawOrigin::Root.into(), 1, 10));
		assert_noop!(
			AccountFreezer::freeze_account(RawOrigin::Root.into(), 1, 10),
			Error::<Test>::AlreadyFrozen
		);
	});
}

#[test]
fn non_root_cannot_freeze() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			AccountFreezer::freeze_account(RawOrigin::Signed(2).into(), 1, 10),
			frame_support::error::BadOrigin
		);
	});
}

#[test]
fn force_origin_can_thaw_early() {
	new_test_ext().execute_with(|| {
		assert_ok!(AccountFreezer::freeze_account(RawOrigin::Root.into(), 1, 1000));
		assert_ok!(AccountFreezer::thaw_account(RawOrigin::Root.into(), 1));
		assert_noop!(
			AccountFreezer::thaw_account(RawOrigin::Root.into(), 1),
			Error::<Test>::NotFrozen
		);
	});
}
