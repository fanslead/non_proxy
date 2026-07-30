use crate::{AppIdentity, Destination, NetworkProfileId};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConnectionContext {
    app: AppIdentity,
    destination: Destination,
    network_profile_id: Option<NetworkProfileId>,
}

impl ConnectionContext {
    #[must_use]
    pub const fn new(app: AppIdentity, destination: Destination) -> Self {
        Self {
            app,
            destination,
            network_profile_id: None,
        }
    }

    #[must_use]
    pub fn with_network_profile(mut self, profile_id: NetworkProfileId) -> Self {
        self.network_profile_id = Some(profile_id);
        self
    }

    #[must_use]
    pub const fn app(&self) -> &AppIdentity {
        &self.app
    }

    #[must_use]
    pub const fn destination(&self) -> &Destination {
        &self.destination
    }

    #[must_use]
    pub const fn network_profile_id(&self) -> Option<&NetworkProfileId> {
        self.network_profile_id.as_ref()
    }
}
