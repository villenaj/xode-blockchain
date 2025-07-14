use crate::{
	AccountId, AllPalletsWithSystem, Balances, ParachainInfo, ParachainSystem, PolkadotXcm,
	Runtime, RuntimeCall, RuntimeEvent, RuntimeOrigin, WeightToFee, XcmpQueue,
	Assets, Balance
};
use frame_support::{
	parameter_types,
	traits::{ConstU32, Contains, Everything, Nothing, ContainsPair},
	weights::Weight,
};
use frame_system::EnsureRoot;
use pallet_xcm::XcmPassthrough;
use polkadot_parachain_primitives::primitives::Sibling;
use polkadot_runtime_common::impls::ToAuthor;
use xcm::latest::prelude::*;
use xcm_builder::{
	AccountId32Aliases, AllowExplicitUnpaidExecutionFrom, AllowTopLevelPaidExecutionFrom,
	DenyReserveTransferToRelayChain, DenyThenTry, EnsureXcmOrigin, FixedWeightBounds, // FungibleAdapter, IsConcrete, NativeAsset
	FrameTransactionalProcessor, ParentIsPreset,
	RelayChainAsNative, SiblingParachainAsNative, SiblingParachainConvertsVia,
	SignedAccountId32AsNative, SignedToAccountId32, SovereignSignedViaLocation, TakeWeightCredit,
	TrailingSetTopicAsId, UsingComponents, WithComputedOrigin, WithUniqueTopic,
	FungiblesAdapter, LocalMint, AllowSubscriptionsFrom
};
use xcm_executor::{
	traits::{Error as MatchError, MatchesFungibles, WeightTrader}, 
	XcmExecutor, AssetsInHolding
};
use sp_core::Get;
use alloc::sync::Arc;

parameter_types! {
	pub const RelayLocation: Location = Location::parent();
	pub const RelayNetwork: Option<NetworkId> = None;
	pub RelayChainOrigin: RuntimeOrigin = cumulus_pallet_xcm::Origin::Relay.into();
	// For the real deployment, it is recommended to set `RelayNetwork` according to the relay chain
	// and prepend `UniversalLocation` with `GlobalConsensus(RelayNetwork::get())`.
	pub UniversalLocation: InteriorLocation = Parachain(ParachainInfo::parachain_id().into()).into();
	/// The account used to perform checks or hold assets during XCM execution,
	/// such as temporary crediting/debiting when receiving or sending assets.
	pub CheckingAccount: AccountId = PolkadotXcm::check_account();
}

/// Type for specifying how a `Location` can be converted into an `AccountId`. This is used
/// when determining ownership of accounts for asset transacting and when attempting to use XCM
/// `Transact` in order to determine the dispatch Origin.
pub type LocationToAccountId = (
	// The parent (Relay-chain) origin converts to the parent `AccountId`.
	ParentIsPreset<AccountId>,
	// Sibling parachain origins convert to AccountId via the `ParaId::into`.
	SiblingParachainConvertsVia<Sibling, AccountId>,
	// Straight up local `AccountId32` origins just alias directly to `AccountId`.
	AccountId32Aliases<RelayNetwork, AccountId>,
);

/// A custom matcher that converts an incoming `Asset` from an XCM message into a local asset ID (`u32`) and amount (`Balance`).
/// 
/// This matcher is used by the XCM asset transactor to interpret location `Asset` objects 
/// and translate them to known local assets that can be used within the chain’s runtime.
pub struct AssetMatcher;
impl MatchesFungibles<u32, Balance> for AssetMatcher {
    fn matches_fungibles(asset: &Asset) -> Result<(u32, Balance), xcm_executor::traits::Error> {
		let match_result = match asset {
			// XCM Inbound - Relay Chain native asset (e.g., KSM)
            Asset {
                id: AssetId(Location {
                    parents: 1,
                    interior: Junctions::Here,
                }),
                fun: Fungibility::Fungible(amount),
            } => {
                log::trace!(target: "xcm::matches_fungibles", "AssetMatcher: Matched Relay Chain native asset: amount = {:?}", amount);
				Ok((100_000_000, *amount))
			},

			// XCM Inbound - Sibling parachain asset (e.g., AssetHub)
            Asset {
                id: AssetId(Location {
                    parents: 1,
                    interior: Junctions::X3(junctions),
                }),
                fun: Fungibility::Fungible(amount),
            } => {
				match junctions.as_ref() {
					[Junction::Parachain(1000), Junction::PalletInstance(50), Junction::GeneralIndex(asset_id)] => {
						log::trace!(target: "xcm::matches_fungibles", "AssetMatcher: Matched AssetHub asset → asset_id: {:?}, amount: {:?}", asset_id, amount);
						Ok((*asset_id as u32, *amount))
					},
					_ => Err(MatchError::AssetNotHandled),
				}
			},

			// XCM Outbound - Local parachain asset (e.g., Xode)
            Asset {
                id: AssetId(Location {
                    parents: 0,
                    interior: Junctions::X2(junctions),
                }),
                fun: Fungibility::Fungible(amount),
            } => {
				match junctions.as_ref() {
					[Junction::PalletInstance(50), Junction::GeneralIndex(asset_id)] => {
						log::trace!(target: "xcm::matches_fungibles", "AssetMatcher: Matched local parachain asset → asset_id: {:?}, amount: {:?}", asset_id, amount);
						Ok((*asset_id as u32, *amount))
					},
					_ => Err(MatchError::AssetNotHandled),
				}
			},

			// Otherwise, mismatched asset type
            _ => Err(MatchError::AssetNotHandled),
        };

		log::trace!(target: "xcm::matches_fungibles", "AssetMatcher: Final result for asset {:?} → {:?}", asset, match_result);
		match_result
    }
}

/// The `AssetTransactor` defines how the runtime handles fungible assets received or sent via XCM.
///
/// It interprets incoming `Asset` locations, resolves them to local asset IDs and balances,
/// and executes operations such as minting, burning, or transferring tokens.
///
/// This implementation uses a custom `AssetMatcher` and supports parachain or AssetHub assets,
/// but not native Relay Chain tokens (handled by a separate transactor if needed).
pub type AssetTransactor = FungiblesAdapter<
    // The asset handler used to inspect, mint, and burn tokens (e.g., orml-tokens or pallet-assets).
    Assets,
    // Custom matcher for converting incoming `Asset` to local (asset ID, balance) pairs.
    // This supports Relay Chain, sibling parachains, or AssetHub assets.
    AssetMatcher,
    // Resolves `Location` origin accounts into native `AccountId`s.
    LocationToAccountId,
    // Native account identifier type used by the runtime.
    AccountId,
    // Handles minting tokens when assets arrive via XCM.
    // NonZeroIssuance ensures no minting of zero-valued assets.
    LocalMint<parachains_common::impls::NonZeroIssuance<AccountId, Assets>>,
    // The system account used for internal checks during XCM asset handling.
    // Prevents unwanted account creation unless explicitly allowed by policies.
    CheckingAccount
>;

/// This filter determines whether a given asset and its origin location are considered "trusted reserve assets"
/// for XCM reserve operations. Only assets and origins that match the trusted patterns will be treated as reserves.
pub struct TrustedReserveAssets;
impl ContainsPair<Asset, Location> for TrustedReserveAssets {
	fn contains(asset: &Asset, origin: &Location) -> bool {
		match &origin {
			// Match the relay chain (parent) as a trusted reserve asset.
			Location { 
				parents: 1, 
				interior: Junctions::Here 
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
			},

			// Match a sibling parachain (e.g., AssetHub with ParaId 1000) as a trusted reserve asset.
			Location { 
				parents: 1, 
				interior: Junctions::X1(parachain_junction)
			} => {
				match parachain_junction.as_ref() {
					[Junction::Parachain(1000)] => {
						if let AssetId(Location { 
							parents: 1, 
							interior: Junctions::X3(asset_junctions) 
						}) = &asset.id {
							let result = matches!(
								asset_junctions.as_ref(),
								[Junction::Parachain(1000), Junction::PalletInstance(50), Junction::GeneralIndex(_)]
							);
							log::trace!(target: "xcm::contains_pair", "TrustedReserveAssets::contains - AssetHub → asset: {:?}, origin: {:?}, result: {:?}", asset, origin, result);
							
							result
						} else {
							false
						}
					},
					_ => false
				}
			},

			// Any other origin or asset combination is not considered a trusted reserve asset and will return `false`.
			_ => false
		}
	}
}

/// This is the type we use to convert an (incoming) XCM origin into a local Origin instance,
/// ready for dispatching a transaction with Xcm's Transact. There is an OriginKind which can
/// biases the kind of local Origin it will become.
pub type XcmOriginToTransactDispatchOrigin = (
	// Sovereign account converter; this attempts to derive an AccountId from the origin location
	// using LocationToAccountId and then turn that into the usual Signed origin. Useful for
	// foreign chains who want to have a local sovereign account on this chain which they control.
	SovereignSignedViaLocation<LocationToAccountId, RuntimeOrigin>,
	// Native converter for Relay-chain (Parent) location; will convert to a Relay origin when
	// recognized.
	RelayChainAsNative<RelayChainOrigin, RuntimeOrigin>,
	// Native converter for sibling Parachains; will convert to a SiblingPara origin when
	// recognized.
	SiblingParachainAsNative<cumulus_pallet_xcm::Origin, RuntimeOrigin>,
	// Native signed account converter; this just converts an AccountId32 origin into a normal
	// RuntimeOrigin::Signed origin of the same 32-byte value.
	SignedAccountId32AsNative<RelayNetwork, RuntimeOrigin>,
	// Xcm origins can be represented natively under the Xcm pallet's Xcm origin.
	XcmPassthrough<RuntimeOrigin>,
);

parameter_types! {
	// One XCM operation is 1_000_000_000 weight - almost certainly a conservative estimate.
	pub UnitWeightCost: Weight = Weight::from_parts(1_000_000_000, 64 * 1024);
	pub const MaxInstructions: u32 = 100;
	pub const MaxAssetsIntoHolding: u32 = 64;
}

/// This struct defines a filter that matches the parent (relay chain) or its executive plurality.
/// It is used to allow XCM operations from the parent chain or its executive body.
pub struct ParentOrTrustedSiblings;
impl Contains<Location> for ParentOrTrustedSiblings {
    fn contains(location: &Location) -> bool {
		match location.unpack() {
			// Parent (relay chain)
			(1, []) => {
				log::trace!(target: "xcm::contains", "ParentOrTrustedSiblings: Matched parent | location: {:?}", location);
				true
			},

			// Parent's executive plurality
			(1, [Junction::Plurality { id, .. }]) if *id == BodyId::Executive => {
				log::trace!(target: "xcm::contains", "ParentOrTrustedSiblings: Matched executive plurality | location: {:?}", location);
				true
			},

			// Any sibling parachain
			(1, [Junction::Parachain(id)]) => {
				log::trace!(target: "xcm::contains", "ParentOrTrustedSiblings: Matched sibling parachain {:?} | location: {:?}", id, location);
				true
			},
			
			// Fallback
			_ => false
		}
    }
}

pub type Barrier = TrailingSetTopicAsId<
	DenyThenTry<
		DenyReserveTransferToRelayChain,
		(
			TakeWeightCredit,
			WithComputedOrigin<
				(
					AllowTopLevelPaidExecutionFrom<Everything>,
					// New: Enables XCM execution requests from sibling parachains.
					AllowExplicitUnpaidExecutionFrom<ParentOrTrustedSiblings>,
					// New: Enables XCM subscription requests from any origin.
					// This is useful for allowing remote chains to subscribe to events or updates from this chain.
					AllowSubscriptionsFrom<Everything>,
				),
				UniversalLocation,
				ConstU32<8>,
			>,
		),
	>,
>;

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
					_ => Err(XcmError::InvalidLocation),
				}
			}
			
			// No match: asset not supported
			_ => {
				Err(XcmError::TooExpensive)
			}
		}
	}

	fn refund_weight(&mut self, _weight: Weight, _context: &XcmContext) -> Option<Asset> {
		None
	}
}

pub struct XcmConfig;
impl xcm_executor::Config for XcmConfig {
	type RuntimeCall = RuntimeCall;
	type XcmSender = XcmRouter;
	// How to withdraw and deposit an asset.
	// type AssetTransactor = LocalAssetTransactor;
	type AssetTransactor = AssetTransactor;
	type OriginConverter = XcmOriginToTransactDispatchOrigin;
	// type IsReserve = NativeAsset;
	type IsReserve = TrustedReserveAssets;
	type IsTeleporter = (); // Teleporting is disabled.
	type UniversalLocation = UniversalLocation;
	type Barrier = Barrier;
	type Weigher = FixedWeightBounds<UnitWeightCost, RuntimeCall, MaxInstructions>;
	// type Trader = UsingComponents<WeightToFee, RelayLocation, AccountId, Balances, ToAuthor<Runtime>>;
	type Trader = DynamicWeightTrader;
	type ResponseHandler = PolkadotXcm;
	type AssetTrap = PolkadotXcm;
	type AssetClaims = PolkadotXcm;
	type SubscriptionService = PolkadotXcm;
	type PalletInstancesInfo = AllPalletsWithSystem;
	type MaxAssetsIntoHolding = MaxAssetsIntoHolding;
	type AssetLocker = ();
	type AssetExchanger = ();
	type FeeManager = ();
	type MessageExporter = ();
	type UniversalAliases = Nothing;
	type CallDispatcher = RuntimeCall;
	type SafeCallFilter = Everything;
	type Aliasers = Nothing;
	type TransactionalProcessor = FrameTransactionalProcessor;
	type HrmpNewChannelOpenRequestHandler = ();
	type HrmpChannelAcceptedHandler = ();
	type HrmpChannelClosingHandler = ();
	type XcmRecorder = PolkadotXcm;
}

/// No local origins on this chain are allowed to dispatch XCM sends/executions.
pub type LocalOriginToLocation = SignedToAccountId32<RuntimeOrigin, AccountId, RelayNetwork>;

/// The means for routing XCM messages which are not for local execution into the right message
/// queues.
pub type XcmRouter = WithUniqueTopic<(
	// Two routers - use UMP to communicate with the relay chain:
	cumulus_primitives_utility::ParentAsUmp<ParachainSystem, (), ()>,
	// ..and XCMP to communicate with the sibling chains.
	XcmpQueue,
)>;

impl pallet_xcm::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type SendXcmOrigin = EnsureXcmOrigin<RuntimeOrigin, LocalOriginToLocation>;
	type XcmRouter = XcmRouter;
	type ExecuteXcmOrigin = EnsureXcmOrigin<RuntimeOrigin, LocalOriginToLocation>;
	// type XcmExecuteFilter = Nothing;
	type XcmExecuteFilter = Everything;
	// ^ Disable dispatchable execute on the XCM pallet.
	// Needs to be `Everything` for local testing.
	type XcmExecutor = XcmExecutor<XcmConfig>;
	type XcmTeleportFilter = Everything;
	// type XcmReserveTransferFilter = Nothing;
	type XcmReserveTransferFilter = Everything;
	type Weigher = FixedWeightBounds<UnitWeightCost, RuntimeCall, MaxInstructions>;
	type UniversalLocation = UniversalLocation;
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;

	const VERSION_DISCOVERY_QUEUE_SIZE: u32 = 100;
	// ^ Override for AdvertisedXcmVersion default
	type AdvertisedXcmVersion = pallet_xcm::CurrentXcmVersion;
	type Currency = Balances;
	type CurrencyMatcher = ();
	type TrustedLockers = ();
	type SovereignAccountOf = LocationToAccountId;
	type MaxLockers = ConstU32<8>;
	type WeightInfo = pallet_xcm::TestWeightInfo;
	type AdminOrigin = EnsureRoot<AccountId>;
	type MaxRemoteLockConsumers = ConstU32<0>;
	type RemoteLockConsumerIdentifier = ();
}

impl cumulus_pallet_xcm::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type XcmExecutor = XcmExecutor<XcmConfig>;
}
