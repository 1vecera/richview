//! Provider-independent email verification for self-registration. A caller installs a delivery
//! adapter; without one there is no registration endpoint and no email is sent. Do not expose
//! this to public sign-up until organization isolation and membership provisioning are in place:
//! the existing server's `org` visibility otherwise includes every signed-in person.
use crate::access::{Access, AccessError, User};
use std::{future::Future, pin::Pin, sync::Arc};

/// Send a single verification link. Implementations must not log the link or token.
pub trait VerificationSender: Send + Sync {
    fn send<'a>(
        &'a self,
        email: &'a str,
        link: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<(), ()>> + Send + 'a>>;
}

/// Orchestrates the verification step; organization assignment and web routes are separate.
pub struct Registration {
    access: Access,
    sender: Arc<dyn VerificationSender>,
    origin: reqwest::Url,
}

#[derive(Debug, PartialEq, Eq)]
pub enum RegistrationError {
    Access(AccessError),
    Delivery,
}

impl From<AccessError> for RegistrationError {
    fn from(value: AccessError) -> Self {
        Self::Access(value)
    }
}

impl Registration {
    pub fn new(
        access: Access,
        sender: Arc<dyn VerificationSender>,
        origin: &str,
    ) -> Result<Self, String> {
        if access.is_hosted() {
            return Err("Hosted registration is not enabled until organization provisioning and link delivery are connected".into());
        }
        let origin = reqwest::Url::parse(origin).map_err(|e| e.to_string())?;
        if origin.scheme() != "https"
            || origin.host_str().is_none()
            || !origin.username().is_empty()
            || origin.password().is_some()
            || origin.path() != "/"
            || origin.query().is_some()
            || origin.fragment().is_some()
        {
            return Err(
                "Registration requires an HTTPS app origin without a path or credentials".into(),
            );
        }
        Ok(Self {
            access,
            sender,
            origin,
        })
    }

    /// Request a verification email without revealing whether an account already exists.
    /// A password is chosen only after the recipient proves control of the address. A web caller
    /// must give `Ok` and `Delivery` the same response, report delivery failures to operators,
    /// and keep the verification URL out of proxy logs and referrers.
    pub async fn request(&self, email: &str) -> Result<(), RegistrationError> {
        let Some((email, token)) = self.access.reserve_registration(email)? else {
            return Ok(());
        };
        let mut link = self
            .origin
            .join("/signup/verify")
            .map_err(|_| RegistrationError::Delivery)?;
        link.query_pairs_mut().append_pair("token", &token);
        if self.sender.send(&email, link.as_str()).await.is_err() {
            self.access.cancel_registration(&token)?;
            return Err(RegistrationError::Delivery);
        }
        Ok(())
    }

    /// Consume the emailed token once and set the password the email recipient chooses.
    pub async fn complete(
        &self,
        token: &str,
        name: &str,
        password: &str,
    ) -> Result<User, RegistrationError> {
        self.access
            .complete_registration(token, name, password)
            .await
            .map_err(Into::into)
    }
}
