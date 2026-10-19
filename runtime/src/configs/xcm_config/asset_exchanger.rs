use crate::{
    AccountId, AssetConversion, Runtime,
    configs::{NativeAndNonPoolAssets, xcm_config::asset_matcher::AssetKindMatcher},
};
use xcm::latest::prelude::*;
use xcm_builder::SingleAssetExchangeAdapter;
use xcm_executor::{traits::{AssetExchange, MatchesFungibles}, AssetsInHolding};

/// Backs XCM's `ExchangeAsset` instruction with the local AMM (`pallet_asset_conversion`),
/// letting an XCM message swap an asset "along the way" - e.g. arrive from another chain,
/// get swapped here, and continue on to a beneficiary in the same message - without a
/// separate extrinsic. `SingleAssetExchangeAdapter` (from `xcm-builder`) does the actual
/// swap via `pallet_asset_conversion`'s `SwapCredit`/`QuotePrice` traits.
type Delegate = SingleAssetExchangeAdapter<AssetConversion, NativeAndNonPoolAssets, AssetKindMatcher, AccountId>;

/// `pallet_asset_waiting` gates `pallet_asset_conversion`'s swap extrinsics behind a
/// `BaseCallFilter` check, which only intercepts calls dispatched through `RuntimeCall`.
/// `ExchangeAsset` calls `SwapCredit` directly from the XCM executor, bypassing that filter
/// entirely - so without this wrapper, a pool still awaiting governance approval would be
/// freely tradeable via XCM even though it's blocked for ordinary signed callers. This
/// reuses `pallet_asset_waiting`'s own `ensure_path_active` check before delegating.
pub struct GatedAssetExchanger;

impl AssetExchange for GatedAssetExchanger {
    fn exchange_asset(
        origin: Option<&Location>,
        give: AssetsInHolding,
        want: &Assets,
        maximal: bool,
    ) -> Result<AssetsInHolding, AssetsInHolding> {
        if !Self::path_active(&give, want) {
            return Err(give);
        }
        Delegate::exchange_asset(origin, give, want, maximal)
    }

    fn quote_exchange_price(give: &Assets, want: &Assets, maximal: bool) -> Option<Assets> {
        Delegate::quote_exchange_price(give, want, maximal)
    }
}

impl GatedAssetExchanger {
    fn path_active(give: &AssetsInHolding, want: &Assets) -> bool {
        let Some(give_asset) = give.fungible_assets_iter().next() else { return false };
        let Some(want_asset) = want.get(0) else { return false };
        let Ok((give_kind, _)) = AssetKindMatcher::matches_fungibles(&give_asset) else { return false };
        let Ok((want_kind, _)) = AssetKindMatcher::matches_fungibles(want_asset) else { return false };
        pallet_asset_waiting::Pallet::<Runtime>::ensure_path_active(&[give_kind, want_kind]).is_ok()
    }
}
