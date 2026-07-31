use thiserror::Error;

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum AdapterIntegrationError {
    #[error("适配器主配置超过大小上限")]
    ConfigurationTooLarge,
    #[error("适配器主配置不是受支持的格式")]
    ConfigurationInvalid,
    #[error("适配器接入标识无效")]
    IntegrationIdInvalid,
    #[error("托管规则文件必须位于主配置目录内")]
    ManagedPathOutsideConfiguration,
    #[error("托管规则引用包含客户端不支持的字符")]
    ManagedPathInvalid,
    #[error("适配器接入节点与现有用户配置冲突")]
    IntegrationConflict,
    #[error("未找到唯一可用的直连出口")]
    DirectTargetAmbiguous,
    #[error("指定的直连出口不存在或不是 direct")]
    DirectTargetInvalid,
}
