#![cfg_attr(not(feature = "std"), no_std)]
extern crate alloc;

use crate::{
    AccountId,
    AllPalletsWithSystem,
    Assets,
    AssetRegistry,
    Balance,
    Balances,
    ParachainInfo,
    ParachainSystem,
    PolkadotXcm,
    Runtime,
    RuntimeCall,
    RuntimeEvent,
    RuntimeOrigin,
    XcmpQueue,
    PoolAssets,

    // XCM config modules
    // configs::xcm_config::asset_matcher::{NativeAssetMatcher, MultiAssetMatcher},
    // configs::xcm_config::trusted_reserve_assets::TrustedReserveAssets,
    // configs::xcm_config::origin_filters::ParentOrTrustedSiblings,
    // configs::xcm_config::weight_trader::DynamicWeightTrader,

    UNIT,
    configs::{XodeTreasuryAccount, EnsureTwoThirdsTechnicalCommittee},
    MessageQueue,
};
use crate::weights;
use core::marker::PhantomData;
use cumulus_primitives_core::{AggregateMessageOrigin, GlobalConsensus, ParaId};
use cumulus_primitives_utility::XcmFeesTo32ByteAccount;
use frame_support::{
	pallet_prelude::{Get, PalletInfoAccess, Weight},
	parameter_types,
	traits::{
		Contains, ContainsPair, Everything, LinearStoragePrice, Nothing, TransformOrigin,
		fungible::HoldConsideration,
	},
};
use frame_system::EnsureRoot;
use orml_traits::{
	location::{RelativeReserveProvider, Reserve},
	parameter_type_with_key,
};
use orml_xcm_support::IsNativeConcrete;
use pallet_xcm::XcmPassthrough;
use parachains_common::{AssetIdForTrustBackedAssets, message_queue::ParaIdToSibling};
use parity_scale_codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use polkadot_parachain_primitives::primitives::Sibling;
use polkadot_runtime_common::xcm_sender::NoPriceForMessageDelivery;
use scale_info::TypeInfo;
use sp_core::ConstU32;
use sp_runtime::{RuntimeDebug, traits::Convert};
use sp_std::{
	convert::{From, Into},
	prelude::*,
};
use xcm::latest::prelude::*;
#[allow(deprecated)]
use xcm_builder::CurrencyAdapter;
use xcm_builder::{
	AccountId32Aliases, AllowKnownQueryResponses, AllowSubscriptionsFrom,
	AllowTopLevelPaidExecutionFrom, AllowUnpaidExecutionFrom, Case,
	DenyReserveTransferToRelayChain, DenyThenTry, DescribeAllTerminal, DescribeFamily,
	EnsureXcmOrigin, FixedRateOfFungible, FixedWeightBounds, FrameTransactionalProcessor,
	FungiblesAdapter, HashedDescription, NoChecking, ParentAsSuperuser, ParentIsPreset,
	RelayChainAsNative, SiblingParachainAsNative, SiblingParachainConvertsVia,
	SignedAccountId32AsNative, SignedToAccountId32, SovereignSignedViaLocation, TakeWeightCredit,
	TrailingSetTopicAsId, WithComputedOrigin,
};
use xcm_executor::{XcmExecutor, traits::JustTry};
use xcm_primitives::{AsAssetLocation, ConvertedRegisteredAssetId};

parameter_types! {
	pub const MaxInstructions: u32 = 100;
}

/// Supported local Currencies. Keep this to XON,
/// other assets will be handled through AssetRegistry pallet
#[derive(
	Encode,
	Decode,
	DecodeWithMemTracking,
	Eq,
	PartialEq,
	Copy,
	Clone,
	RuntimeDebug,
	PartialOrd,
	Ord,
	TypeInfo,
	MaxEncodedLen,
)]
pub enum CurrencyId {
	UNIT,
}

/// Converts a Locaction into a CurrencyId. Used by XCMP LocalAssetTransactor for asset
/// filtering: we only accept Assets that are convertable to a "CurrencyId".
/// other assets will be handled through AssetRegistry pallet
impl Convert<Location, Option<CurrencyId>> for CurrencyIdConvert {
	fn convert(location: Location) -> Option<CurrencyId> {
		let self_para_id: u32 = ParachainInfo::parachain_id().into();

		match location.unpack() {
			(1, [Parachain(id), XON_GENERAL_KEY]) if *id == self_para_id => Some(CurrencyId::UNIT),
			(1, [Parachain(id)]) if *id == self_para_id => Some(CurrencyId::UNIT),
			(0, [XON_GENERAL_KEY]) => Some(CurrencyId::UNIT),
			(0, []) => Some(CurrencyId::UNIT),
			_ => None,
		}
	}
}

/// Converts an Asset into a CurrencyId by checking its Location.
impl Convert<Asset, Option<CurrencyId>> for CurrencyIdConvert {
	fn convert(asset: Asset) -> Option<CurrencyId> {
		Self::convert(asset.id.0)
	}
}

parameter_types! {
    pub const RelayLocation: Location = Location::parent();
    pub AssetHubLocation: Location = Location::new(1, [Parachain(1000)]);
    pub const RelayNetwork: NetworkId = Polkadot;
    pub RelayChainOrigin: RuntimeOrigin = cumulus_pallet_xcm::Origin::Relay.into();
    // For the real deployment, it is recommended to set `RelayNetwork` according to the relay chain
    // and prepend `UniversalLocation` with `GlobalConsensus(RelayNetwork::get())`.
    pub UniversalLocation: InteriorLocation =
		[GlobalConsensus(RelayNetwork::get()), Parachain(ParachainInfo::parachain_id().into())].into();
    /// The account used to perform checks or hold assets during XCM execution,
    /// such as temporary crediting/debiting when receiving or sending assets.
    // pub const TokenLocation: Location = Location::parent();
    pub TrustBackedAssetsPalletLocation: Location =
		PalletInstance(<PoolAssets as PalletInfoAccess>::index() as u8).into();
	// pub TrustBackedAssetsPalletIndex: u8 = <Assets as PalletInfoAccess>::index() as u8;
    // pub PoolAssetsPalletLocation: Location =
	// 	PalletInstance(<PoolAssets as PalletInfoAccess>::index() as u8).into();
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
    // Foreign locations alias into accounts according to a hash of their standard description.
	HashedDescription<AccountId, DescribeFamily<DescribeAllTerminal>>,
);

/// The asset transactor for handling the local native asset.
/// 
/// This supports only the native token of this parachain.
/// It uses the `Balances` pallet to manage the native currency.
#[allow(deprecated)]
pub type LocalAssetTransactor = CurrencyAdapter<
    // The asset handler for the native currency (Balances pallet).
	Balances,
    // Our custom asset matcher for the native token.
	// NativeAssetMatcher,
    IsNativeConcrete<CurrencyId, CurrencyIdConvert>,
    // Resolves `Location` origin accounts into native `AccountId`s.
	LocationToAccountId,
	// Our chain's account ID type (we can't get away without mentioning it explicitly):
	AccountId,
	// We don't track any teleports.
	(),
>;


/// `AssetId/Balancer` converter for `TrustBackedAssets`
pub type TrustBackedAssetsConvertedConcreteId =
	assets_common::TrustBackedAssetsConvertedConcreteId<TrustBackedAssetsPalletLocation, Balance>;


/// The asset transactor for handling assets via pallet-assets.
/// 
/// This supports assets from the Relay Chain, sibling parachains (e.g., AssetHub),
/// and local pallet-assets defined on this parachain.
pub type PalletAssetsTransactor = FungiblesAdapter<
    // The asset handler used to inspect, mint, and burn tokens (pallet-assets).
    Assets,
    // Our custom asset matcher for various fungible assets.
    // MultiAssetMatcher,
    TrustBackedAssetsConvertedConcreteId,
    // Resolves `Location` origin accounts into native `AccountId`s.
    LocationToAccountId,
    // Native account identifier type used by the runtime.
    AccountId,
    // Handles minting tokens when assets arrive via XCM.
    // NonZeroIssuance ensures no minting of zero-valued assets.
    NoChecking,
    // LocalMint<parachains_common::impls::NonZeroIssuance<AccountId, Assets>>,
    // The system account used for internal checks during XCM asset handling.
    // Prevents unwanted account creation unless explicitly allowed by policies.
    CheckingAccount,
>;

pub type ForeignAssetTransactor = FungiblesAdapter<
    Assets,
    ConvertedRegisteredAssetId<
        AssetIdForTrustBackedAssets,
        Balance,
        AsAssetLocation<AssetIdForTrustBackedAssets, AssetRegistry>,
        JustTry,
    >,
    LocationToAccountId,
    AccountId,
    // Use LocalMint to mint assets locally when they arrive
    NoChecking,
    // LocalMint<parachains_common::impls::NonZeroIssuance<AccountId, Assets>>,
    CheckingAccount,
>;

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
    // Superuser converter for the Relay-chain (Parent) location. This will allow it to issue a
	// transaction from the Root origin.
	ParentAsSuperuser<RuntimeOrigin>,
    // Native signed account converter; this just converts an AccountId32 origin into a normal
    // RuntimeOrigin::Signed origin of the same 32-byte value.
    SignedAccountId32AsNative<RelayNetwork, RuntimeOrigin>,
    // Xcm origins can be represented natively under the Xcm pallet's Xcm origin.
    XcmPassthrough<RuntimeOrigin>,
);


pub struct ParentOrParentsExecutivePlurality;
impl Contains<Location> for ParentOrParentsExecutivePlurality {
	fn contains(location: &Location) -> bool {
		matches!(location.unpack(), (1, []) | (1, [Plurality { id: BodyId::Executive, .. }]))
	}
}

pub struct ParentOrSiblings;
impl Contains<Location> for ParentOrSiblings {
	fn contains(location: &Location) -> bool {
		matches!(location.unpack(), (1, []) | (1, _))
	}
}


pub type Barrier = TrailingSetTopicAsId<
    DenyThenTry<
        DenyReserveTransferToRelayChain,
        (
            TakeWeightCredit,
            AllowKnownQueryResponses<PolkadotXcm>,
            WithComputedOrigin<
                (
                    AllowTopLevelPaidExecutionFrom<Everything>,
                    // New: Enables XCM execution requests from sibling parachains.
                    AllowUnpaidExecutionFrom<ParentOrParentsExecutivePlurality>,
                    // New: Enables XCM subscription requests from any origin.
                    // This is useful for allowing remote chains to subscribe to events or updates from this chain.
                    AllowSubscriptionsFrom<ParentOrSiblings>,
                ),
                UniversalLocation,
                ConstU32<8>,
            >,
        ),
    >,
>;

pub struct ReserveAssetsFrom<T>(PhantomData<T>);
impl<T: Get<Location>> ContainsPair<Asset, Location> for ReserveAssetsFrom<T> {
	fn contains(asset: &Asset, origin: &Location) -> bool {
		let prefix = T::get();
		log::trace!(target: "xcm::AssetsFrom", "prefix: {prefix:?}, origin: {origin:?}, asset: {asset:?}");
		&prefix == origin
	}
}

pub type Traders = (
	FixedRateOfFungible<
		NativePerSecond,
		XcmFeesTo32ByteAccount<LocalAssetTransactor, AccountId, XodeTreasuryAccount>,
	>,
	FixedRateOfFungible<
		NativeAliasPerSecond,
		XcmFeesTo32ByteAccount<LocalAssetTransactor, AccountId, XodeTreasuryAccount>,
	>,
	FixedRateOfFungible<
		RelayNativePerSecond,
		XcmFeesTo32ByteAccount<ForeignAssetTransactor, AccountId, XodeTreasuryAccount>,
	>,
);

/// The overall asset transactor for XCM, combining local native asset handling
/// and pallet-assets handling for other fungible assets.
pub type AssetTransactors = (
    LocalAssetTransactor,
    ForeignAssetTransactor,
    PalletAssetsTransactor,
);



parameter_type_with_key! {
	pub ParachainMinFee: |_location: Location| -> Option<u128> {
		None
	};
}

const fn xon_general_key() -> Junction {
	const XON_KEY: [u8; 32] = *b"XON\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0";
	GeneralKey { length: 3, data: XON_KEY }
}
const XON_GENERAL_KEY: Junction = xon_general_key();

parameter_types! {
    // One XCM operation is 1_000_000_000 weight - almost certainly a conservative estimate.
    pub UnitWeightCost: Weight = Weight::from_parts(1_000_000_000, 64 * 1024);
    pub const MaxAssetsIntoHolding: u32 = 64;
    pub NativePerSecond: (AssetId, u128,u128) = (Location::new(0,Here).into(), UNIT * 70, 0u128);
    pub NativeAliasPerSecond: (AssetId, u128,u128) = (Location::new(0,[XON_GENERAL_KEY]).into(), UNIT * 70, 0u128);
	pub RelayNativePerSecond: (AssetId, u128,u128) = (Location::new(1,Here).into(), UNIT * 70, 0u128);
    pub RelayLocationFilter: AssetFilter = Wild(AllOf {
		fun: WildFungible,
		id: AssetId(RelayLocation::get()),
	});
    pub RelayChainNativeAssetFromAssetHub: (AssetFilter, Location) = (
		RelayLocationFilter::get(),
		AssetHubLocation::get()
	);
}



/// Converts a CurrencyId into a Location, used by xtoken for XCMP.
pub struct CurrencyIdConvert;
impl Convert<CurrencyId, Option<Location>> for CurrencyIdConvert {
	fn convert(id: CurrencyId) -> Option<Location> {
		match id {
			CurrencyId::UNIT => Some(Location::new(
				1,
				[Parachain(ParachainInfo::parachain_id().into()), XON_GENERAL_KEY],
			)),
		}
	}
}







pub type Reserves = (
    Case<RelayChainNativeAssetFromAssetHub>,
    ReserveAssetsFrom<AssetHubLocation>, 
);



pub struct XcmConfig;
impl xcm_executor::Config for XcmConfig {
    type RuntimeCall = RuntimeCall;
    type XcmSender = XcmRouter;
    // How to withdraw and deposit an asset.
    type AssetTransactor = AssetTransactors;
    type OriginConverter = XcmOriginToTransactDispatchOrigin;
    type IsReserve = Reserves;
    // type IsReserve = TrustedReserveAssets;
    type IsTeleporter = (); // Teleporting is disabled.
    type UniversalLocation = UniversalLocation;
    type Barrier = Barrier;
    type Weigher = FixedWeightBounds<UnitWeightCost, RuntimeCall, MaxInstructions>;
    type Trader = Traders;
    // type Trader = DynamicWeightTrader;
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
    // Stable 2512 Update
    type XcmEventEmitter = PolkadotXcm;
}

/// No local origins on this chain are allowed to dispatch XCM sends/executions.
pub type LocalOriginToLocation = SignedToAccountId32<RuntimeOrigin, AccountId, RelayNetwork>;

/// The means for routing XCM messages which are not for local execution into the right message
/// queues.
pub type XcmRouter = (
    // Two routers - use UMP to communicate with the relay chain:
    cumulus_primitives_utility::ParentAsUmp<ParachainSystem, PolkadotXcm, ()>,
    // ..and XCMP to communicate with the sibling chains.
    XcmpQueue,
);

impl pallet_xcm::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type SendXcmOrigin = EnsureXcmOrigin<RuntimeOrigin, LocalOriginToLocation>;
    type XcmRouter = XcmRouter;
    type ExecuteXcmOrigin = EnsureXcmOrigin<RuntimeOrigin, LocalOriginToLocation>;
    type XcmExecuteFilter = Everything;
    // ^ Disable dispatchable execute on the XCM pallet.
    // Needs to be `Everything` for local testing.
    type XcmExecutor = XcmExecutor<XcmConfig>;
    type XcmTeleportFilter = Everything;
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
    // Stable 2512 Update
    type AuthorizedAliasConsideration = ();
}

impl cumulus_pallet_xcm::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type XcmExecutor = XcmExecutor<XcmConfig>;
}

/// Copied from moonbeam: https://github.com/PureStake/moonbeam/blob/095031d171b0c163e5649ee35acbc36eef681a82/primitives/xcm/src/ethereum_xcm.rs#L34
pub const DEFAULT_PROOF_SIZE: u64 = 1024;

parameter_types! {
	pub const BaseXcmWeight: Weight= Weight::from_parts(1_000_000u64, DEFAULT_PROOF_SIZE);
	pub const MaxAssetsForTransfer: usize = 2;
}

parameter_types! {
	pub SelfReserveAlias: Location = Location::new(
		0,
		[XON_GENERAL_KEY]
	);
	// This is how we are going to detect whether the asset is a Reserve asset
	pub SelfLocation: Location = Location::here();
	// We need this to be able to catch when someone is trying to execute a non-
	// cross-chain transfer in xtokens through the absolute path way
	pub SelfLocationAbsolute: Location = Location::new(
		1,
		Parachain(ParachainInfo::parachain_id().into())
	);

}

/// This struct offers uses RelativeReserveProvider to output relative views of Locations
/// However, additionally accepts a Location that aims at representing the chain part
/// (parent: 1, Parachain(paraId)) of the absolute representation of our chain.
/// If a token reserve matches against this absolute view, we return  Some(Location::here())
/// This helps users by preventing errors when they try to transfer a token through xtokens
/// to our chain (either inserting the relative or the absolute value).
pub struct AbsoluteAndRelativeReserve<AbsoluteLocation>(PhantomData<AbsoluteLocation>);
impl<AbsoluteLocation> Reserve for AbsoluteAndRelativeReserve<AbsoluteLocation>
where
	AbsoluteLocation: Get<Location>,
{
	fn reserve(asset: &Asset) -> Option<Location> {
		RelativeReserveProvider::reserve(asset).map(|relative_reserve| {
			if relative_reserve == AbsoluteLocation::get() {
				Location::here()
			} else {
				relative_reserve
			}
		})
	}
}

pub struct AccountIdToLocation;
impl Convert<AccountId, Location> for AccountIdToLocation {
	fn convert(account: AccountId) -> Location {
		[AccountId32 { network: None, id: account.into() }].into()
	}
}

impl orml_xcm::Config for Runtime {
	type SovereignOrigin = EnsureRoot<AccountId>;
}

impl orml_xtokens::Config for Runtime {
	type Balance = Balance;
	type CurrencyId = CurrencyId;
	type CurrencyIdConvert = CurrencyIdConvert;
	type AccountIdToLocation = AccountIdToLocation;
	type SelfLocation = SelfLocation;
	type MinXcmFee = ParachainMinFee;
	type XcmExecutor = XcmExecutor<XcmConfig>;
	type LocationsFilter = Everything;
	type Weigher = FixedWeightBounds<UnitWeightCost, RuntimeCall, MaxInstructions>;
	type BaseXcmWeight = BaseXcmWeight;
	type UniversalLocation = UniversalLocation;
	type MaxAssetsForTransfer = MaxAssetsForTransfer;
	type ReserveProvider = AbsoluteAndRelativeReserve<SelfLocationAbsolute>;
	type RateLimiter = ();
	type RateLimiterId = ();
}

impl cumulus_pallet_xcmp_queue::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type ChannelInfo = ParachainSystem;
	type VersionWrapper = ();
	// Enqueue XCMP messages from siblings for later processing.
	type XcmpQueue = TransformOrigin<MessageQueue, AggregateMessageOrigin, ParaId, ParaIdToSibling>;
	type MaxInboundSuspended = sp_core::ConstU32<1_000>;
	type MaxActiveOutboundChannels = ConstU32<128>;
	type MaxPageSize = ConstU32<{ 1 << 16 }>;
	type ControllerOrigin = EnsureTwoThirdsTechnicalCommittee;
	type ControllerOriginConverter = XcmOriginToTransactDispatchOrigin;
	type WeightInfo = ();
	type PriceForSiblingDelivery = NoPriceForMessageDelivery<ParaId>;
}