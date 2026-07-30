pub mod common {
    pub mod v1 {
        tonic::include_proto!("nonproxy.common.v1");
    }
}

pub mod policy {
    pub mod v1 {
        tonic::include_proto!("nonproxy.policy.v1");
    }
}

pub mod events {
    pub mod v1 {
        tonic::include_proto!("nonproxy.events.v1");
    }
}

pub mod control {
    pub mod v1 {
        tonic::include_proto!("nonproxy.control.v1");
    }
}

pub mod provider {
    pub mod v1 {
        tonic::include_proto!("nonproxy.provider.v1");
    }
}
