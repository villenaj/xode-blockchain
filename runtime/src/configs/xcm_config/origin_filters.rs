use frame_support::traits::Contains;
use xcm::latest::prelude::*;

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
            }

            // Parent's executive plurality
            (1, [Junction::Plurality { id, .. }]) if *id == BodyId::Executive => {
                log::trace!(target: "xcm::contains", "ParentOrTrustedSiblings: Matched executive plurality | location: {:?}", location);
                true
            }

            // Any sibling parachain
            (1, [Junction::Parachain(id)]) => {
                log::trace!(target: "xcm::contains", "ParentOrTrustedSiblings: Matched sibling parachain {:?} | location: {:?}", id, location);
                true
            }

            // Fallback
            _ => {
                log::trace!(target: "xcm::contains", "ParentOrTrustedSiblings: No match | location: {:?}", location);
                false
            }
        }
    }
}
