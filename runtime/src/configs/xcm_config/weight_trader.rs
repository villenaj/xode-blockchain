use crate::{configs::xcm_config::AssetTransactors};
use frame_support::{
    parameter_types,
    weights::{Weight, WeightToFee as WeightToFeeT},
};
use xcm::latest::prelude::*;
use xcm_executor::{
	AssetsInHolding,
	traits::{TransactAsset, WeightTrader}
};
use alloc::sync::Arc;
use sp_core::crypto::{Ss58Codec, AccountId32};
use core::marker::PhantomData;

parameter_types! {
	pub const RelayLocation: Location = Location::parent();
}

/// Trait defining parameters for weight-to-fee conversion for different assets.
/// 
/// Implementers of this trait specify the fee rate per second for a given asset,
/// allowing the `WeightToFeeConverter` to calculate the appropriate fee based on
/// the weight of the XCM execution.
pub trait WeightToFeeAssetParams {
	const FEE_PER_SECOND: u128;
}

/// Weight-to-fee parameters for XON (native token) with a rate of 0.01 XON per second.
pub struct XonWeightToFeeRate;
impl WeightToFeeAssetParams for XonWeightToFeeRate {
	const FEE_PER_SECOND: u128 = 10_000_000_000; 
}

/// Weight-to-fee parameters for DOT (relay chain token) with a rate of 0.01 DOT per second.
pub struct DotWeightToFeeRate;
impl WeightToFeeAssetParams for DotWeightToFeeRate {
	const FEE_PER_SECOND: u128 = 100_000_000; // 0.01 DOT
}

/// Weight-to-fee parameters for USDT (AssetHub token) with a rate of 0.01 USDT per second.
pub struct UsdtWeightToFeeRate;
impl WeightToFeeAssetParams for UsdtWeightToFeeRate {
	const FEE_PER_SECOND: u128 = 10_000; // 0.01 USDT
}

/// A generic weight-to-fee converter that calculates the fee based on the weight
/// of the XCM execution and the fee rate defined by the implementer of
/// `WeightToFeeAssetParams`.
///
/// This struct uses the `WeightToFeeAssetParams` trait to determine the fee rate
/// for the specific asset, allowing for flexible fee calculations based on the
/// asset being used for payment.
pub struct WeightToFeeConverter<T: WeightToFeeAssetParams>(PhantomData<T>);

impl<T: WeightToFeeAssetParams> WeightToFeeT for WeightToFeeConverter<T> {
	type Balance = u128;

	fn weight_to_fee(weight: &Weight) -> Self::Balance {
    let picos_per_second: u128 = 1_000_000_000_000u128;
    let ref_time = weight.ref_time() as u128;
    
    // Perform multiplication in u128 to avoid overflow
    let total_fee = ref_time.saturating_mul(T::FEE_PER_SECOND) / picos_per_second;
    total_fee
  }
}

/// Dynamic weight trader that calculates fees based on the asset used for payment.
/// 
/// It supports XON (native token), DOT (relay chain token), and USDT (AssetHub token).
/// The fee is calculated based on the weight of the XCM execution and a fixed fee
/// of 0.01 units of the respective asset.
/// 
/// This implementation deducts the required fee from the provided assets and deposits
/// it to a predefined fee collector account.
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
		log::trace!(target: "xcm::weight_trader", "DynamicWeightTrader::buy_weight - weight: {:?}, payment: {:?}, context: {:?}",  weight, payment, context);

		// Early return if payment is empty
		let (asset_id, _) = payment.fungible.iter().next().ok_or(XcmError::TooExpensive)?;

		// Check if asset matches USDT on AssetHub (parachain 1000)
		match asset_id {
			AssetId(xon_location @ Location {
				parents: 0,
				interior: Junctions::Here,
			}) => handle_payment(xon_location.clone(), payment, WeightToFeeConverter::<XonWeightToFeeRate>::weight_to_fee(&weight)),

			AssetId(dot_location @ Location {
				parents: 1,
				interior: Junctions::Here,
			}) => handle_payment(dot_location.clone(), payment, WeightToFeeConverter::<DotWeightToFeeRate>::weight_to_fee(&weight)),

			AssetId(usdt_location @ Location {
				parents: 1,
				interior: Junctions::X3(junctions),
			}) if matches!(
				junctions.as_ref(),
				[
					Junction::Parachain(1000),
					Junction::PalletInstance(50),
					Junction::GeneralIndex(1984)
				]
			) => handle_payment(usdt_location.clone(), payment, WeightToFeeConverter::<UsdtWeightToFeeRate>::weight_to_fee(&weight)),
			
			_ => {
				log::trace!(target: "xcm::weight_trader", "DynamicWeightTrader::buy_weight - Unsupported asset: {:?}", asset_id);
				Err(XcmError::AssetNotFound)
			}
		}
	}

	fn refund_weight(&mut self, _weight: Weight, _context: &XcmContext) -> Option<Asset> {
		None
	}
}

/// Handles the payment for weight purchase by deducting the required fee
/// from the provided assets and depositing it to the fee collector.
/// 
/// The function calculates the total fee based on the provided `fee_amount`
/// and a fixed fee of 0.01 units of the asset. It then deducts this total fee
/// from the `payment` assets. If successful, it deposits the fee to a predefined
/// fee collector account and returns any remaining assets.
fn handle_payment(
	asset_location: Location,
	payment: AssetsInHolding,
	fee_amount: u128,
) -> Result<AssetsInHolding, XcmError> {
	let fixed_fee: u128 = match asset_location.clone() {
		// XON: local chain (12 decimals -> 0.01 XON = 10_000_000_000)
		Location {
			parents: 0,
			interior: Junctions::Here,
		} => 10_000_000_000,

		// DOT: relay chain (10 decimals -> 0.01 DOT = 100_000_000)
		Location {
			parents: 1,
			interior: Junctions::Here,
		} => 100_000_000,

		// USDT: AssetHub (6 decimals -> 0.01 USDT = 10_000)
		Location {
			parents: 1,
			interior: Junctions::X3(junctions),
		} if matches!(
			junctions.as_ref(),
			[
				Junction::Parachain(1000),
				Junction::PalletInstance(50),
				Junction::GeneralIndex(1984)
			]
		) => 10_000,

		_ => {
			log::error!(
				target: "xcm::weight_trader", "handle_payment - Unsupported asset location: {:?}",
				asset_location
			);
			return Err(XcmError::FailedToTransactAsset("Unsupported asset location"));
		}
	};

	let total_fee = fee_amount.saturating_add(fixed_fee);

	// Subtract total fee from payment
	let required_asset: Asset = (AssetId(asset_location.clone()), total_fee).into();
	let unused_assets = payment.checked_sub(required_asset).map_err(|_| XcmError::TooExpensive)?;

	// Remaining balance
	let (_, remaining_balance) = unused_assets.fungible.iter().next().unwrap_or((&AssetId(asset_location.clone()), &0));

	// Receiver (fee collector)
	let receiver = AccountId32::from_ss58check("5Fqc6tG16FcR4QLwknqs5sb9Pc6pqsNwtwcPh7fNKCaP4DJT")
		.map_err(|e| {
			log::error!(target: "xcm::weight_trader", "Invalid receiver address: {:?}", e);
			XcmError::FailedToTransactAsset("Invalid receiver address")
		})?;

	let receiver_location = Location {
		parents: 0,
		interior: Junctions::X1(Arc::from([Junction::AccountId32 {
			network: None,
			id: receiver.into(),
		}])),
	};

	// Deposit total fee
	let fee_asset: Asset = (AssetId(asset_location.clone()), total_fee).into();
	AssetTransactors::deposit_asset(&fee_asset, &receiver_location, None).map_err(|e| {
		log::error!(target: "xcm::weight_trader", "Fee deposit failed for {:?}: {:?}", asset_location, e);
		XcmError::FailedToTransactAsset("Fee deposit failed")
	})?;

	// Return remaining assets (if any)
	let mut final_assets = AssetsInHolding::new();
	if *remaining_balance > 0 {
		final_assets.subsume((AssetId(asset_location.clone()), *remaining_balance).into());
	}

	log::trace!(
		target: "xcm::weight_trader", "handle_payment - Processed {:?}: total_fee={:?} (+0.01), remaining={:?}",
		asset_location, total_fee, remaining_balance
	);

	Ok(final_assets)
}