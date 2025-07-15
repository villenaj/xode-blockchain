use crate::{
	AccountId, Balances, Runtime, WeightToFee
};
use frame_support::{
    parameter_types,
    weights::{Weight, WeightToFee as WeightToFeeT}
};
use polkadot_runtime_common::impls::ToAuthor;
use xcm::latest::prelude::*;
use xcm_builder::UsingComponents;
use xcm_executor::{traits::WeightTrader, AssetsInHolding};
use alloc::sync::Arc;

parameter_types! {
	pub const RelayLocation: Location = Location::parent();
}

// Weight to fee conversion for USDT
/// This implementation assumes that 1 unit of weight corresponds to 1_000 micro-USDT
/// (or 0.001 USDT). Adjust this based on your actual fee structure.
/// 
/// This is used in the `DynamicWeightTrader` to convert weight to a fee in US
pub struct UsdtWeightToFee;

impl WeightToFeeT for UsdtWeightToFee {
	type Balance = u128;

	fn weight_to_fee(weight: &Weight) -> Self::Balance {
		weight.ref_time().saturating_div(1_000_000).max(1).into()
	}
}

/// A dynamic weight trader that can handle multiple asset types for weight purchasing.
/// This trader can be used to buy weight using different assets based on the context
/// and available assets.
/// 
/// This is useful for scenarios where the asset used for weight purchase may vary
/// based on the XCM message or the context of the transaction.
/// 
/// This implementation allows for dynamic handling of weight purchasing
/// based on the assets available in the `AssetsInHolding` and the context of the XCM message.
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
					// Match AssetHub asset with ParaId 1000 and PalletInstance 50
					[Junction::Parachain(1000), Junction::PalletInstance(50), Junction::GeneralIndex(_)] => {
						log::trace!(target: "xcm::weight_trader", "DynamicWeightTrader::buy_weight - AssetHub asset junctions: {:?}", junctions);

						let usdt = 1984u32;
						let fee_amount = UsdtWeightToFee::weight_to_fee(&weight);

    					log::trace!(target: "xcm::weight_trader", "DynamicWeightTrader::buy_weight - Using USDT for weight purchase: {:?}", fee_amount);
    
						let required_asset_payment: Asset = (
							AssetId(Location {
								parents: 1,
								interior: Junctions::X3(Arc::from([
									Junction::Parachain(1000),
									Junction::PalletInstance(50),
									Junction::GeneralIndex(usdt as u128),
								])),
							}),
							fee_amount,
						).into();
						let unused = payment.checked_sub(required_asset_payment).map_err(|_| XcmError::TooExpensive)?;

						log::trace!(target: "xcm::weight_trader", "DynamicWeightTrader::buy_weight - Successfully purchased weight with USDT: {:?}", fee_amount);

						Ok(unused)
					},

					// If junctions do not match expected AssetHub format
					_ => {
						log::trace!(target: "xcm::weight_trader", "DynamicWeightTrader::buy_weight - AssetHub asset junctions mismatch: {:?}", junctions);
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
