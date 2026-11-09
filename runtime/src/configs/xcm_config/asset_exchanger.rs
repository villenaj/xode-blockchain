//! XCM `ExchangeAsset` support, backed by the on-chain DEX (`pallet-asset-conversion`).
//!
//! This lets a cross-chain message swap assets on Xode's pools, e.g. USDT sent from
//! Asset Hub is swapped to XON on arrival, with one signature on the sending chain.
//!
//! Two pieces are Xode-specific:
//!
//! 1. [`SwapAssetMatcher`]: XCM and `pallet-asset-conversion` name the same assets
//!    differently on Xode.
//!
//!    | Asset          | XCM location (holding)            | DEX `AssetKind`                 |
//!    |----------------|-----------------------------------|---------------------------------|
//!    | XON (native)   | `{0, Here}`                       | `{1, Here}` (`TokenLocation`)   |
//!    | Asset Hub USDT | `{1, [Para 1000, Pallet 50, 1984]}` | `{0, [Pallet 50, Index 1984]}` |
//!    | Local asset N  | `{0, [Pallet 50, Index N]}`       | `{0, [Pallet 50, Index N]}`     |
//!    | DOT            | `{1, Here}`                       | `{0, [Pallet 50, 100_000_000]}` |
//!
//!    The matcher never re-implements these rules: it asks the same matchers the asset
//!    transactor uses ([`NativeAssetMatcher`], [`MultiAssetMatcher`]) which underlying
//!    balance an XCM asset is, then names that balance the way the DEX does. So the
//!    exchanger always swaps exactly the balance the holding register stands for.
//!
//! 2. [`GatedAssetExchanger`]: the DEX is otherwise gated by the runtime call filter
//!    (`TxPause` and `AssetWaiting`). XCM calls the DEX directly and would skip that
//!    filter, so the same checks are applied here.

use crate::{AccountId, AssetConversion, AssetWaiting, Balance, TxPause};
use crate::configs::NativeAndNonPoolAssets;
use super::{
    asset_matcher::{MultiAssetMatcher, NativeAssetMatcher},
    TokenLocation, TrustBackedAssetsPalletIndex,
};
use alloc::vec::Vec;
use core::marker::PhantomData;
use frame_support::traits::PalletInfoAccess;
use xcm::latest::prelude::*;
use xcm_builder::SingleAssetExchangeAdapter;
use xcm_executor::{
    traits::{AssetExchange, Error as MatchError, MatchesFungible, MatchesFungibles},
    AssetsInHolding,
};

/// Maps an XCM asset to the DEX `AssetKind` for the same underlying balance.
pub struct SwapAssetMatcher;

impl MatchesFungibles<Location, Balance> for SwapAssetMatcher {
    fn matches_fungibles(asset: &Asset) -> Result<(Location, Balance), MatchError> {
        // Native XON (Balances). The DEX keys the native currency as `TokenLocation`.
        if let Some(amount) = NativeAssetMatcher::matches_fungible(asset) {
            return Ok((TokenLocation::get(), amount));
        }

        // Anything held in `pallet-assets` (local, Asset Hub reserve-backed, or DOT).
        // The DEX keys these by their local `pallet-assets` location.
        let (asset_id, amount) = MultiAssetMatcher::matches_fungibles(asset)?;
        let dex_asset = Location::new(
            0,
            [
                PalletInstance(TrustBackedAssetsPalletIndex::get()),
                GeneralIndex(asset_id.into()),
            ],
        );

        Ok((dex_asset, amount))
    }
}

/// The DEX-backed exchanger, without the governance gate.
pub type DexAssetExchanger =
    SingleAssetExchangeAdapter<AssetConversion, NativeAndNonPoolAssets, SwapAssetMatcher, AccountId>;

/// Applies the runtime call filter's DEX rules to XCM exchanges, then defers to `Inner`.
///
/// An exchange is refused (`NoDeal`, the asset stays in holding) when:
/// - the matching `AssetConversion` swap call is paused through `TxPause`, or
/// - the pool is not **Active** in `AssetWaiting` (pending pools can't be traded).
pub struct GatedAssetExchanger<Inner>(PhantomData<Inner>);

impl<Inner> GatedAssetExchanger<Inner> {
    /// The extrinsic an exchange stands in for: `maximal` spends all of `give`
    /// (exact input), otherwise it buys exactly `want` (exact output).
    fn swap_call_name(maximal: bool) -> &'static [u8] {
        if maximal {
            b"swap_exact_tokens_for_tokens"
        } else {
            b"swap_tokens_for_exact_tokens"
        }
    }

    fn is_allowed(give: &Asset, want: &Asset, maximal: bool) -> bool {
        let pallet_name = <AssetConversion as PalletInfoAccess>::name().as_bytes().to_vec();
        if TxPause::is_paused_unbound(pallet_name, Self::swap_call_name(maximal).to_vec()) {
            log::debug!(target: "xcm::asset_exchanger", "Refused: AssetConversion swaps are paused");
            return false;
        }

        let (Ok((give_id, _)), Ok((want_id, _))) = (
            SwapAssetMatcher::matches_fungibles(give),
            SwapAssetMatcher::matches_fungibles(want),
        ) else {
            return false;
        };

        let path: Vec<Location> = alloc::vec![give_id, want_id];
        if AssetWaiting::ensure_path_active(&path).is_err() {
            log::debug!(target: "xcm::asset_exchanger", "Refused: pool is not Active → path: {:?}", path);
            return false;
        }

        true
    }
}

impl<Inner: AssetExchange> AssetExchange for GatedAssetExchanger<Inner> {
    fn exchange_asset(
        origin: Option<&Location>,
        give: AssetsInHolding,
        want: &Assets,
        maximal: bool,
    ) -> Result<AssetsInHolding, AssetsInHolding> {
        // `Inner` only supports one fungible asset on each side; anything else is
        // left to it to refuse.
        let allowed = match (give.fungible_assets_iter().next(), want.get(0)) {
            (Some(give_asset), Some(want_asset)) => Self::is_allowed(&give_asset, want_asset, maximal),
            _ => true,
        };
        if !allowed {
            return Err(give);
        }

        Inner::exchange_asset(origin, give, want, maximal)
    }

    fn quote_exchange_price(give: &Assets, want: &Assets, maximal: bool) -> Option<Assets> {
        let (give_asset, want_asset) = (give.get(0)?, want.get(0)?);
        if !Self::is_allowed(give_asset, want_asset, maximal) {
            return None;
        }

        Inner::quote_exchange_price(give, want, maximal)
    }
}

/// The `AssetExchanger` used by the XCM executor.
pub type XodeAssetExchanger = GatedAssetExchanger<DexAssetExchanger>;

#[cfg(test)]
mod tests {
    //! These run the real `XcmExecutor<XcmConfig>` against the real runtime pallets,
    //! with a XON/USDT pool seeded in fresh storage.

    use super::*;
    use crate::{
        configs::xcm_config::XcmConfig, Assets, Balances, Runtime, RuntimeOrigin, UNIT,
    };
    use frame_support::{
        assert_ok,
        traits::fungibles::Inspect as _,
        weights::Weight,
    };
    use sp_io::TestExternalities;
    use sp_runtime::{AccountId32 as RawAccountId, BuildStorage};
    use xcm::latest::{InstructionError, Outcome};
    use xcm_executor::XcmExecutor;

    const USDT: u32 = 1984;
    const USDT_UNIT: Balance = 1_000_000;
    const POOL_XON: Balance = 1_000 * UNIT;
    const POOL_USDT: Balance = 100 * USDT_UNIT; // 1 USDT ≈ 10 XON

    fn alice() -> AccountId { RawAccountId::new([1u8; 32]) }
    fn bob() -> AccountId { RawAccountId::new([2u8; 32]) }

    fn account(who: AccountId) -> Location {
        Location::new(0, [AccountId32 { network: None, id: who.into() }])
    }
    fn asset_hub() -> Location { Location::new(1, [Parachain(1000)]) }
    /// USDT as Asset Hub (its reserve) names it; what arrives in holding.
    fn usdt_xcm() -> Location {
        Location::new(1, [Parachain(1000), PalletInstance(50), GeneralIndex(USDT.into())])
    }
    fn usdt_dex() -> Location {
        Location::new(0, [PalletInstance(TrustBackedAssetsPalletIndex::get()), GeneralIndex(USDT.into())])
    }
    fn xon_xcm() -> Location { Location::here() }

    fn usdt_of(who: &AccountId) -> Balance { Assets::balance(USDT, who) }
    fn xon_of(who: &AccountId) -> Balance { Balances::free_balance(who) }

    /// Fresh storage with USDT (asset 1984) and a XON/USDT pool.
    /// `pending`: create the pool through `AssetWaiting`, so it awaits approval.
    fn new_ext(pending: bool) -> TestExternalities {
        let mut t = frame_system::GenesisConfig::<Runtime>::default().build_storage().unwrap();
        pallet_balances::GenesisConfig::<Runtime> {
            balances: alloc::vec![(alice(), 10_000 * UNIT), (bob(), UNIT)],
            ..Default::default()
        }
        .assimilate_storage(&mut t)
        .unwrap();

        let mut ext = TestExternalities::new(t);
        ext.execute_with(|| {
            frame_system::Pallet::<Runtime>::set_block_number(1);
            let signed = RuntimeOrigin::signed(alice());
            assert_ok!(Assets::create(signed.clone(), USDT.into(), alice().into(), 10_000));
            assert_ok!(Assets::mint(signed.clone(), USDT.into(), alice().into(), 1_000 * USDT_UNIT));

            let (xon, usdt) = (alloc::boxed::Box::new(TokenLocation::get()), alloc::boxed::Box::new(usdt_dex()));
            if pending {
                assert_ok!(AssetWaiting::create_pool(signed.clone(), xon.clone(), usdt.clone()));
            } else {
                assert_ok!(AssetConversion::create_pool(signed.clone(), xon.clone(), usdt.clone()));
            }
            assert_ok!(AssetConversion::add_liquidity(signed, xon, usdt, POOL_XON, POOL_USDT, 1, 1, alice()));
        });
        ext
    }

    fn execute(origin: Location, message: Xcm<crate::RuntimeCall>) -> Result<(), InstructionError> {
        let mut hash = [0u8; 32];
        // Barrier and fee payment already work on-chain; they aren't under test here.
        let outcome: Outcome = XcmExecutor::<XcmConfig>::prepare_and_execute(
            origin, message, &mut hash, Weight::MAX, Weight::MAX,
        );
        outcome.ensure_complete()
    }

    /// USDT arriving from Asset Hub, swapped to at least `min_xon`, deposited to Bob.
    fn inbound_usdt_swap(usdt: Balance, min_xon: Balance) -> Xcm<crate::RuntimeCall> {
        Xcm(alloc::vec![
            ReserveAssetDeposited((usdt_xcm(), usdt).into()),
            ClearOrigin,
            ExchangeAsset {
                give: Wild(AllOf { id: AssetId(usdt_xcm()), fun: WildFungible }),
                want: (xon_xcm(), min_xon).into(),
                maximal: true,
            },
            DepositAsset { assets: Wild(AllCounted(2)), beneficiary: account(bob()) },
        ])
    }

    fn assert_no_deal(result: Result<(), InstructionError>) {
        let err = result.expect_err("exchange should have been refused");
        assert_eq!((err.index, err.error), (2, XcmError::NoDeal));
    }

    #[test]
    fn matcher_maps_xcm_assets_to_dex_keys() {
        let m = |id: Location| SwapAssetMatcher::matches_fungibles(&(id, 5u128).into());

        assert_eq!(m(xon_xcm()), Ok((TokenLocation::get(), 5)));
        assert_eq!(m(usdt_xcm()), Ok((usdt_dex(), 5)));
        assert_eq!(m(usdt_dex()), Ok((usdt_dex(), 5)));
        // DOT is `{1, Here}` in XCM, which is also the DEX key for *XON*. It must map to
        // DOT's own pallet-assets id, never to the native pool leg.
        let dot = m(Location::parent()).unwrap().0;
        assert_ne!(dot, TokenLocation::get());
        assert_eq!(dot, Location::new(0, [PalletInstance(TrustBackedAssetsPalletIndex::get()), GeneralIndex(100_000_000)]));
        // Assets the transactor doesn't handle aren't exchangeable either.
        assert!(m(Location::new(1, [Parachain(2034), PalletInstance(51), GeneralIndex(1)])).is_err());
    }

    #[test]
    fn inbound_usdt_from_asset_hub_is_swapped_to_xon() {
        new_ext(false).execute_with(|| {
            let quote = <XodeAssetExchanger as AssetExchange>::quote_exchange_price(
                &(usdt_xcm(), 2 * USDT_UNIT).into(), &(xon_xcm(), 1u128).into(), true,
            ).expect("quote");
            let quoted_xon = match quote.get(0).unwrap().fun { Fungible(a) => a, _ => unreachable!() };

            let usdt_issuance = Assets::total_issuance(USDT);
            let xon_issuance = Balances::total_issuance();
            let bob_xon = xon_of(&bob());

            assert_ok!(execute(asset_hub(), inbound_usdt_swap(2 * USDT_UNIT, 19 * UNIT)));

            // Bob received the quoted XON (~10 XON per USDT, less the 0.3% fee).
            assert_eq!(xon_of(&bob()) - bob_xon, quoted_xon);
            assert!(quoted_xon >= 19 * UNIT && quoted_xon < 20 * UNIT, "{quoted_xon}");
            assert_eq!(usdt_of(&bob()), 0);
            // Only the reserve-backed USDT was minted; XON just moved out of the pool.
            assert_eq!(Assets::total_issuance(USDT), usdt_issuance + 2 * USDT_UNIT);
            assert_eq!(Balances::total_issuance(), xon_issuance);
        });
    }

    #[test]
    fn local_xon_is_swapped_to_exact_usdt_with_change_returned() {
        new_ext(false).execute_with(|| {
            let (alice_xon, alice_usdt) = (xon_of(&alice()), usdt_of(&alice()));
            let message = Xcm(alloc::vec![
                WithdrawAsset((xon_xcm(), 50 * UNIT).into()),
                ExchangeAsset {
                    give: Wild(AllOf { id: AssetId(xon_xcm()), fun: WildFungible }),
                    want: (usdt_xcm(), USDT_UNIT).into(),
                    maximal: false,
                },
                DepositAsset { assets: Wild(AllCounted(2)), beneficiary: account(alice()) },
            ]);

            assert_ok!(execute(account(alice()), message));

            assert_eq!(usdt_of(&alice()), alice_usdt + USDT_UNIT);
            let spent = alice_xon - xon_of(&alice());
            assert!(spent > 10 * UNIT && spent < 11 * UNIT, "unused XON should be returned, spent {spent}");
        });
    }

    #[test]
    fn slippage_limit_is_enforced() {
        new_ext(false).execute_with(|| {
            let bob_xon = xon_of(&bob());
            assert_no_deal(execute(asset_hub(), inbound_usdt_swap(2 * USDT_UNIT, 20 * UNIT)));
            assert_eq!(xon_of(&bob()), bob_xon);
        });
    }

    #[test]
    fn pending_pool_is_not_tradable_over_xcm() {
        new_ext(true).execute_with(|| {
            assert_no_deal(execute(asset_hub(), inbound_usdt_swap(2 * USDT_UNIT, 1)));
            assert!(<XodeAssetExchanger as AssetExchange>::quote_exchange_price(
                &(usdt_xcm(), USDT_UNIT).into(), &(xon_xcm(), 1u128).into(), true,
            ).is_none());
        });
    }

    #[test]
    fn paused_swaps_are_not_tradable_over_xcm() {
        new_ext(false).execute_with(|| {
            let name = |s: &str| -> pallet_tx_pause::PalletNameOf<Runtime> {
                s.as_bytes().to_vec().try_into().unwrap()
            };
            pallet_tx_pause::PausedCalls::<Runtime>::insert(
                (name("AssetConversion"), name("swap_exact_tokens_for_tokens")), (),
            );
            assert_no_deal(execute(asset_hub(), inbound_usdt_swap(2 * USDT_UNIT, 1)));
        });
    }

    #[test]
    fn dex_pallet_names_match_tx_pause_keys() {
        // Guards the byte strings in `GatedAssetExchanger` against renames.
        assert_eq!(<AssetConversion as PalletInfoAccess>::name(), "AssetConversion");
        let _ = pallet_asset_conversion::Call::<Runtime>::swap_exact_tokens_for_tokens {
            path: Default::default(), amount_in: 0, amount_out_min: 0, send_to: alice(), keep_alive: false,
        };
        let _ = pallet_asset_conversion::Call::<Runtime>::swap_tokens_for_exact_tokens {
            path: Default::default(), amount_out: 0, amount_in_max: 0, send_to: alice(), keep_alive: false,
        };
    }
}
