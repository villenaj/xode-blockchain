use crate::Balance;
use frame_support::traits::ContainsPair;
use xcm::latest::prelude::*;
use xcm_executor::traits::{Error as MatchError, MatchesFungibles};

/// A custom matcher that converts an incoming `Asset` from an XCM message into a local asset ID (`u32`) and amount (`Balance`).
///
/// This matcher is used by the XCM asset transactor to interpret location `Asset` objects
/// and translate them to known local assets that can be used within the chain’s runtime.
pub struct AssetMatcher;

impl MatchesFungibles<u32, Balance> for AssetMatcher {
    fn matches_fungibles(asset: &Asset) -> Result<(u32, Balance), MatchError> {
        let match_result = match asset {
            // XCM Inbound - Relay Chain native asset (e.g., KSM)
            Asset {
                id:
                    AssetId(Location {
                        parents: 1,
                        interior: Junctions::Here,
                    }),
                fun: Fungibility::Fungible(amount),
            } => {
                log::trace!(target: "xcm::matches_fungibles", "AssetMatcher: Matched Relay Chain native asset: amount = {:?}", amount);
                Ok((100_000_000, *amount))
            }

            // XCM Inbound - Sibling parachain asset (e.g., AssetHub)
            Asset {
                id:
                    AssetId(Location {
                        parents: 1,
                        interior: Junctions::X3(junctions),
                    }),
                fun: Fungibility::Fungible(amount),
            } => match junctions.as_ref() {
                [Junction::Parachain(1000), Junction::PalletInstance(50), Junction::GeneralIndex(asset_id)] =>
                {
                    log::trace!(target: "xcm::matches_fungibles", "AssetMatcher: Matched AssetHub asset → asset_id: {:?}, amount: {:?}", asset_id, amount);
                    Ok((*asset_id as u32, *amount))
                }
                _ => Err(MatchError::AssetNotHandled),
            },

            // XCM Outbound - Local parachain asset (e.g., Xode)
            Asset {
                id:
                    AssetId(Location {
                        parents: 0,
                        interior: Junctions::X2(junctions),
                    }),
                fun: Fungibility::Fungible(amount),
            } => match junctions.as_ref() {
                [Junction::PalletInstance(50), Junction::GeneralIndex(asset_id)] => {
                    log::trace!(target: "xcm::matches_fungibles", "AssetMatcher: Matched local parachain asset → asset_id: {:?}, amount: {:?}", asset_id, amount);
                    Ok((*asset_id as u32, *amount))
                }
                _ => Err(MatchError::AssetNotHandled),
            },

            // Otherwise, mismatched asset type
            _ => {
                log::trace!(target: "xcm::matches_fungibles", "AssetMatcher: Asset not handled → asset: {:?}", asset);
                Err(MatchError::AssetNotHandled)
            }
        };

        log::trace!(target: "xcm::matches_fungibles", "AssetMatcher: Final result for asset {:?} → {:?}", asset, match_result);
        match_result
    }
}

/// This filter determines whether a given asset and its origin location are considered "trusted reserve assets"
/// for XCM reserve operations. Only assets and origins that match the trusted patterns will be treated as reserves.
pub struct TrustedReserveAssets;

impl ContainsPair<Asset, Location> for TrustedReserveAssets {
    fn contains(asset: &Asset, origin: &Location) -> bool {
        match &origin {
            // Match the relay chain (parent) as a trusted reserve asset.
            Location {
                parents: 1,
                interior: Junctions::Here,
            } => {
                let result = matches!(
                    &asset.id,
                    AssetId(Location {
                        parents: 1,
                        interior: Junctions::Here
                    })
                );
                log::trace!(target: "xcm::contains_pair", "TrustedReserveAssets::contains - RelayChain → asset: {:?}, origin: {:?}, result: {:?}", asset, origin, result);

                result
            }

            // Match a sibling parachain (e.g., AssetHub with ParaId 1000) as a trusted reserve asset.
            Location {
                parents: 1,
                interior: Junctions::X1(parachain_junction),
            } => match parachain_junction.as_ref() {
                [Junction::Parachain(1000)] => {
                    if let AssetId(Location {
                        parents: 1,
                        interior: Junctions::X3(asset_junctions),
                    }) = &asset.id
                    {
                        let result = matches!(
                            asset_junctions.as_ref(),
                            [
                                Junction::Parachain(1000),
                                Junction::PalletInstance(50),
                                Junction::GeneralIndex(_)
                            ]
                        );
                        log::trace!(target: "xcm::contains_pair", "TrustedReserveAssets::contains - AssetHub → asset: {:?}, origin: {:?}, result: {:?}", asset, origin, result);

                        result
                    } else {
                        log::trace!(target: "xcm::contains_pair", "TrustedReserveAssets::contains - AssetHub mismatch → asset: {:?}, origin: {:?}", asset, origin);
                        false
                    }
                }
                _ => {
                    log::trace!(target: "xcm::contains_pair", "TrustedReserveAssets::contains - Non-AssetHub sibling parachain → asset: {:?}, origin: {:?}", asset, origin);
                    false
                }
            },

            // Any other origin or asset combination is not considered a trusted reserve asset and will return `false`.
            _ => {
                log::trace!(target: "xcm::contains_pair", "TrustedReserveAssets::contains - Other → asset: {:?}, origin: {:?}", asset, origin);
                false
            }
        }
    }
}
