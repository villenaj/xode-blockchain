//! Exercises `GatedAssetExchanger` (the `AssetExchanger` wired into `XcmConfig`) directly through
//! `XcmExecutor`, without any XCMP/UMP transport - a synthetic `Xcm` program is executed as if it
//! had just arrived, covering the one property that matters here: a pool still awaiting governance
//! approval (`pallet_asset_waiting`) must stay untradeable via `ExchangeAsset`, exactly as it is for
//! the equivalent `pallet_asset_conversion` extrinsics, and a pool that has cleared approval must
//! become tradeable.

use crate::{
    AccountId, Assets, AssetWaiting, BalancesConfig, Runtime, RuntimeCall, RuntimeGenesisConfig,
    RuntimeOrigin, System, TechnicalCommitteeMembershipConfig,
    configs::{AssetKind, TechnicalMembershipMaxMembers, xcm_config::XcmConfig},
};
use frame_support::{assert_ok, BoundedVec};
use sp_keyring::Sr25519Keyring;
use sp_runtime::{BuildStorage, MultiAddress};
use xcm::latest::prelude::*;
use xcm_executor::XcmExecutor;

const LOCAL_ASSET: u32 = 2;
/// 1 XON, given `Balance` is expressed with 12 decimals elsewhere in this runtime.
const UNIT: u128 = 1_000_000_000_000;

fn location_of(account: &AccountId) -> Location {
    Location::new(0, [Junction::AccountId32 { network: None, id: account.clone().into() }])
}

fn local_asset_location() -> Location {
    Location::new(0, [Junction::PalletInstance(50), Junction::GeneralIndex(LOCAL_ASSET as u128)])
}

fn new_test_ext(funded: Vec<AccountId>, governance: [AccountId; 2]) -> sp_io::TestExternalities {
    let members = BoundedVec::<AccountId, TechnicalMembershipMaxMembers>::try_from(governance.to_vec())
        .expect("2 members fit the membership bound");

    let genesis = RuntimeGenesisConfig {
        balances: BalancesConfig {
            balances: funded.into_iter().map(|acc| (acc, 1_000_000 * UNIT)).collect(),
            ..Default::default()
        },
        technical_committee_membership: TechnicalCommitteeMembershipConfig {
            members,
            phantom: Default::default(),
        },
        ..Default::default()
    };
    let storage = genesis.build_storage().expect("valid genesis");
    let mut ext = sp_io::TestExternalities::new(storage);
    ext.execute_with(|| System::set_block_number(1));
    ext
}

/// Creates `LOCAL_ASSET`, mints a supply to `user`, and opens a native/local pool via
/// `AssetWaiting::create_pool` (left `Pending` - the caller decides whether to approve it) with
/// liquidity already seeded, so an `ExchangeAsset` swap has something to trade against.
fn setup_pool(user: &AccountId) {
    assert_ok!(Assets::create(
        RuntimeOrigin::signed(user.clone()),
        codec::Compact(LOCAL_ASSET),
        MultiAddress::Id(user.clone()),
        1,
    ));
    assert_ok!(Assets::mint(
        RuntimeOrigin::signed(user.clone()),
        codec::Compact(LOCAL_ASSET),
        MultiAddress::Id(user.clone()),
        1_000 * UNIT,
    ));
    assert_ok!(AssetWaiting::create_pool(
        RuntimeOrigin::signed(user.clone()),
        Box::new(AssetKind::Native),
        Box::new(AssetKind::Local(LOCAL_ASSET)),
    ));
    // Bypasses `BaseCallFilter` (a direct `Pallet::<Runtime>` call, not a `RuntimeCall::dispatch`),
    // which is fine here: seeding liquidity isn't the behaviour under test, and the pool being
    // `Pending` while liquidity is added mirrors how `AssetWaiting::create_pool` already adds the
    // creator's own liquidity via its internal `pallet_asset_conversion::create_pool` call.
    assert_ok!(pallet_asset_conversion::Pallet::<Runtime>::add_liquidity(
        RuntimeOrigin::signed(user.clone()),
        Box::new(AssetKind::Native),
        Box::new(AssetKind::Local(LOCAL_ASSET)),
        100 * UNIT,
        100 * UNIT,
        1,
        1,
        user.clone(),
    ));
}

fn approve_pool(governance: [AccountId; 2]) {
    for approver in governance {
        assert_ok!(AssetWaiting::approve_pool(
            RuntimeOrigin::signed(approver),
            Box::new(AssetKind::Native),
            Box::new(AssetKind::Local(LOCAL_ASSET)),
        ));
    }
}

/// `WithdrawAsset` (native, covering both the swap-in amount and the fee) -> `BuyExecution` ->
/// `ExchangeAsset` (native for `LOCAL_ASSET`) -> `DepositAsset` (beneficiary) - the "swap along the
/// way" program a wallet/router/another chain would send in a real cross-chain swap.
fn exchange_program(swap_in: u128, fee: u128, beneficiary: &AccountId) -> Xcm<RuntimeCall> {
    Xcm(vec![
        WithdrawAsset(vec![(Location::here(), swap_in + fee).into()].into()),
        BuyExecution { fees: (Location::here(), fee).into(), weight_limit: Unlimited },
        ExchangeAsset {
            give: AssetFilter::Definite(vec![(Location::here(), swap_in).into()].into()),
            want: vec![(local_asset_location(), 1u128).into()].into(),
            maximal: false,
        },
        DepositAsset { assets: Wild(All), beneficiary: location_of(beneficiary) },
    ])
}

fn execute(origin: &AccountId, message: Xcm<RuntimeCall>) -> Outcome {
    // Generous fixed cap - well above what 4 instructions cost under `UnitWeightCost`
    // (`FixedWeightBounds`'s per-instruction estimate); we're not testing weight metering here.
    let weight_limit = Weight::from_parts(20_000_000_000, 2_000_000);
    let mut hash = [0u8; 32];
    XcmExecutor::<XcmConfig>::prepare_and_execute(
        location_of(origin),
        message,
        &mut hash,
        weight_limit,
        Weight::zero(),
    )
}

#[test]
fn exchange_asset_is_blocked_while_pool_is_pending() {
    let user = Sr25519Keyring::Alice.to_account_id();
    let governance = [Sr25519Keyring::Bob.to_account_id(), Sr25519Keyring::Charlie.to_account_id()];
    let beneficiary = Sr25519Keyring::Dave.to_account_id();

    new_test_ext(vec![user.clone(), beneficiary.clone()], governance).execute_with(|| {
        setup_pool(&user);

        execute(&user, exchange_program(10 * UNIT, UNIT, &beneficiary));

        assert_eq!(Assets::balance(LOCAL_ASSET, &beneficiary), 0);
    });
}

#[test]
fn exchange_asset_succeeds_once_pool_is_active() {
    let user = Sr25519Keyring::Alice.to_account_id();
    let governance = [Sr25519Keyring::Bob.to_account_id(), Sr25519Keyring::Charlie.to_account_id()];
    let beneficiary = Sr25519Keyring::Dave.to_account_id();

    new_test_ext(vec![user.clone(), beneficiary.clone()], governance.clone()).execute_with(|| {
        setup_pool(&user);
        approve_pool(governance);

        let outcome = execute(&user, exchange_program(10 * UNIT, UNIT, &beneficiary));

        assert!(matches!(outcome, Outcome::Complete { .. }), "expected Complete, got {outcome:?}");
        assert!(Assets::balance(LOCAL_ASSET, &beneficiary) > 0);
    });
}
