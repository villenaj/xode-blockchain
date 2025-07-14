use crate::{
	AccountId, Balances, Runtime, WeightToFee,
};
use frame_support::{
    parameter_types,
    weights::Weight,
};
use polkadot_runtime_common::impls::ToAuthor;
use xcm::latest::prelude::*;
use xcm_builder::UsingComponents;
use xcm_executor::{traits::WeightTrader, AssetsInHolding};
use sp_core::Get;
use alloc::sync::Arc;

parameter_types! {
	pub const RelayLocation: Location = Location::parent();
}

/// A location representing the AssetHub USDT asset, which is used for weight purchasing.
pub struct AssethubUsdtLocation;

impl Get<Location> for AssethubUsdtLocation {
    fn get() -> Location {
        Location {
            parents: 1,
            interior: Junctions::X3(Arc::from([
                Junction::Parachain(1000),
                Junction::PalletInstance(50),
                Junction::GeneralIndex(1984),
            ])),
        }
    }
}

/// A location representing the local USDT asset, which is used for weight purchasing.
pub struct LocalUsdtLocation;

impl Get<Location> for LocalUsdtLocation {
    fn get() -> Location {
        Location {
            parents: 0,
            interior: Junctions::X2(Arc::from([
                Junction::PalletInstance(50),
                Junction::GeneralIndex(1984),
            ])),
        }
    }
}

/// A dynamic weight trader that can handle different asset types for weight purchasing.
/// It uses the `UsingComponents` trait to determine which asset to use based on the
/// asset ID provided in the payment.
/// 
/// This allows for flexible weight purchasing based on the available assets in the holding register.
pub struct DynamicWeightTrader;

impl WeightTrader for DynamicWeightTrader {
	fn new() -> Self {
		Self
	}

	fn buy_weight(
		&mut self,
		weight: Weight,
		payment: AssetsInHolding,
		context: &XcmContext,
	) -> Result<AssetsInHolding, XcmError> {
		// Determine the asset ID to use for weight purchasing.
		let asset_id = payment.fungible.iter().find_map(|(id, _balance)| Some(id.clone()));

		match asset_id {
			// Match Relay Chain native token (e.g., DOT) for weight purchase.
			Some(AssetId(Location {
				parents: 1, 
				interior: Junctions::Here 
			})) => {
				log::trace!(target: "xcm::weight_trader", "DynamicWeightTrader::buy_weight - Relay Chain native asset junctions: {:?}", RelayLocation::get());
				UsingComponents::<
					WeightToFee, 
					RelayLocation, 
					AccountId, 
					Balances, 
					ToAuthor<Runtime>
				>::new().buy_weight(weight, payment, context)
			}

			// Match AssetHub asset junctions (Parachain 1000, PalletInstance 50, GeneralIndex) → treat as USDT
			Some(AssetId(Location {
				parents: 1,
				interior: Junctions::X3(junctions),
			})) => {
				match junctions.as_ref() {
					[Junction::Parachain(1000), Junction::PalletInstance(50), Junction::GeneralIndex(_)] => {
						log::trace!(target: "xcm::weight_trader", "DynamicWeightTrader::buy_weight - AssetHub asset junctions: {:?}", junctions);
						UsingComponents::<
							WeightToFee,
							AssethubUsdtLocation,
							AccountId,
							Balances,
							ToAuthor<Runtime>
						>::new().buy_weight(weight, payment, context)
					},
					_ => Err(XcmError::InvalidLocation),
				}
			}
			
			// Match local asset junctions (PalletInstance 50, GeneralIndex) → treat as USDT
			Some(AssetId(Location {
				parents: 0,
				interior: Junctions::X2(junctions),
			})) => {
				match junctions.as_ref() {
					[Junction::PalletInstance(50), Junction::GeneralIndex(_)] => {
						log::trace!(target: "xcm::weight_trader", "DynamicWeightTrader::buy_weight - Local asset junctions: {:?}", junctions);
						UsingComponents::<
							WeightToFee,
							LocalUsdtLocation,
							AccountId,
							Balances,
							ToAuthor<Runtime>
						>::new().buy_weight(weight, payment, context)
					},
					_ => {
						log::trace!(target: "xcm::weight_trader", "DynamicWeightTrader::buy_weight - Local asset junctions mismatch: {:?}", junctions);
						Err(XcmError::InvalidLocation)
					},
				}
			}
			
			// No match: asset not supported
			_ => {
				log::trace!(target: "xcm::weight_trader", "DynamicWeightTrader::buy_weight - No matching asset for weight purchase: {:?}", asset_id);
				Err(XcmError::TooExpensive)
			}
		}
	}

	fn refund_weight(&mut self, _weight: Weight, _context: &XcmContext) -> Option<Asset> {
		None
	}
}
