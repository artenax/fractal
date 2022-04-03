mod identity_verification;
mod verification_list;

use std::time::Duration;

pub use self::{
    identity_verification::{
        IdentityVerification, Mode as VerificationMode, SasData, State as VerificationState,
        SupportedMethods as VerificationSupportedMethods,
    },
    verification_list::VerificationList,
};
/// The time a verification is valid after its creation.
#[allow(dead_code)]
pub const VERIFICATION_CREATION_TIMEOUT: Duration = Duration::from_secs(60 * 10);
/// The time a verification is valid after it was received by the client.
#[allow(dead_code)]
pub const VERIFICATION_RECEIVE_TIMEOUT: Duration = Duration::from_secs(60 * 2);
