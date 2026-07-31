// swift-tools-version: 6.1

import PackageDescription

let package = Package(
    name: "NonProxyMacPlatform",
    platforms: [
        .macOS(.v15),
    ],
    products: [
        .library(
            name: "NonProxyProviderContracts",
            targets: ["NonProxyProviderContracts"]
        ),
        .library(
            name: "NonProxyProviderCore",
            targets: ["NonProxyProviderCore"]
        ),
        .library(
            name: "NonProxyMacPlatformSupport",
            targets: ["NonProxyMacPlatformSupport"]
        ),
        .library(
            name: "NonProxyMacRuntime",
            targets: ["NonProxyMacRuntime"]
        ),
        .library(
            name: "NonProxyNativeMessaging",
            targets: ["NonProxyNativeMessaging"]
        ),
        .library(
            name: "NonProxyTransparentProxy",
            targets: ["NonProxyTransparentProxy"]
        ),
        .library(
            name: "NonProxyDNSProxy",
            targets: ["NonProxyDNSProxy"]
        ),
        .library(
            name: "NonProxyMacHostBridge",
            type: .dynamic,
            targets: ["NonProxyMacHostBridge"]
        ),
        .executable(
            name: "NonProxyTransparentSystemExtension",
            targets: ["NonProxyTransparentSystemExtension"]
        ),
        .executable(
            name: "NonProxyDNSSystemExtension",
            targets: ["NonProxyDNSSystemExtension"]
        ),
        .executable(
            name: "NonProxyProviderSmoke",
            targets: ["NonProxyProviderSmoke"]
        ),
        .executable(
            name: "NonProxyFlowSmoke",
            targets: ["NonProxyFlowSmoke"]
        ),
        .executable(
            name: "NonProxyNativeMessagingHost",
            targets: ["NonProxyNativeMessagingHost"]
        ),
        .executable(
            name: "NonProxySafariWebExtension",
            targets: ["NonProxySafariWebExtension"]
        ),
        .executable(
            name: "NonProxySafariStateProbe",
            targets: ["NonProxySafariStateProbe"]
        ),
    ],
    dependencies: [
        .package(
            url: "https://github.com/apple/swift-protobuf.git",
            exact: "1.38.1"
        ),
        .package(
            url: "https://github.com/grpc/grpc-swift-2.git",
            exact: "2.4.2"
        ),
        .package(
            url: "https://github.com/grpc/grpc-swift-protobuf.git",
            exact: "2.4.1"
        ),
        .package(
            url: "https://github.com/grpc/grpc-swift-nio-transport.git",
            exact: "2.9.0"
        ),
    ],
    targets: [
        .target(
            name: "NonProxyProviderContracts",
            dependencies: [
                .product(name: "SwiftProtobuf", package: "swift-protobuf"),
                .product(name: "GRPCCore", package: "grpc-swift-2"),
                .product(name: "GRPCProtobuf", package: "grpc-swift-protobuf"),
            ]
        ),
        .target(
            name: "NonProxyProviderCore",
            dependencies: [
                "NonProxyProviderContracts",
                .product(name: "GRPCCore", package: "grpc-swift-2"),
                .product(
                    name: "GRPCNIOTransportHTTP2Posix",
                    package: "grpc-swift-nio-transport"
                ),
                .product(name: "SwiftProtobuf", package: "swift-protobuf"),
            ],
            linkerSettings: [
                .linkedFramework("CryptoKit"),
                .linkedFramework("Network"),
            ]
        ),
        .target(
            name: "NonProxyMacRuntime"
        ),
        .target(
            name: "NonProxyMacNetworkIdentity",
            linkerSettings: [
                .linkedFramework("CoreWLAN"),
                .linkedFramework("CryptoKit"),
                .linkedFramework("Network"),
                .linkedFramework("SystemConfiguration"),
            ]
        ),
        .target(
            name: "NonProxySafariStateBridge",
            publicHeadersPath: "include",
            linkerSettings: [
                .linkedFramework("SafariServices"),
            ]
        ),
        .target(
            name: "NonProxyNativeMessaging",
            dependencies: [
                "NonProxyMacRuntime",
                "NonProxyProviderContracts",
                .product(name: "GRPCCore", package: "grpc-swift-2"),
                .product(
                    name: "GRPCNIOTransportHTTP2Posix",
                    package: "grpc-swift-nio-transport"
                ),
                .product(name: "SwiftProtobuf", package: "swift-protobuf"),
            ]
        ),
        .target(
            name: "NonProxyMacPlatformSupport",
            dependencies: [
                "NonProxyMacNetworkIdentity",
                "NonProxyMacRuntime",
                "NonProxyProviderCore",
                "NonProxyProviderContracts",
            ],
            linkerSettings: [
                .linkedFramework("Network"),
                .linkedFramework("NetworkExtension"),
                .linkedFramework("Security"),
            ]
        ),
        .target(
            name: "NonProxyTransparentProxy",
            dependencies: [
                "NonProxyMacNetworkIdentity",
                "NonProxyMacPlatformSupport",
                "NonProxyProviderCore",
                "NonProxyProviderContracts",
            ],
            linkerSettings: [
                .linkedFramework("Network"),
                .linkedFramework("NetworkExtension"),
            ]
        ),
        .target(
            name: "NonProxyDNSProxy",
            dependencies: [
                "NonProxyMacNetworkIdentity",
                "NonProxyMacPlatformSupport",
                "NonProxyProviderCore",
                "NonProxyProviderContracts",
            ],
            linkerSettings: [
                .linkedFramework("CryptoKit"),
                .linkedFramework("Network"),
                .linkedFramework("NetworkExtension"),
            ]
        ),
        .target(
            name: "NonProxyMacHostBridge",
            dependencies: [
                "NonProxyMacNetworkIdentity",
                "NonProxyMacRuntime",
            ],
            linkerSettings: [
                .linkedFramework("AppKit"),
                .linkedFramework("CoreLocation"),
                .linkedFramework("NetworkExtension"),
                .linkedFramework("Security"),
                .linkedFramework("ServiceManagement"),
                .linkedFramework("SystemConfiguration"),
                .linkedFramework("SystemExtensions"),
                .linkedFramework("UniformTypeIdentifiers"),
            ]
        ),
        .executableTarget(
            name: "NonProxyTransparentSystemExtension",
            dependencies: ["NonProxyTransparentProxy"],
            linkerSettings: [
                .linkedFramework("NetworkExtension"),
            ]
        ),
        .executableTarget(
            name: "NonProxyDNSSystemExtension",
            dependencies: ["NonProxyDNSProxy"],
            linkerSettings: [
                .linkedFramework("NetworkExtension"),
            ]
        ),
        .executableTarget(
            name: "NonProxyProviderSmoke",
            dependencies: [
                "NonProxyProviderCore",
                "NonProxyProviderContracts",
            ]
        ),
        .executableTarget(
            name: "NonProxyFlowSmoke",
            dependencies: ["NonProxyProviderCore"],
            linkerSettings: [
                .linkedFramework("Network"),
            ]
        ),
        .executableTarget(
            name: "NonProxyNativeMessagingHost",
            dependencies: ["NonProxyNativeMessaging"]
        ),
        .executableTarget(
            name: "NonProxySafariWebExtension",
            dependencies: ["NonProxyNativeMessaging"],
            swiftSettings: [
                .unsafeFlags(["-application-extension"]),
            ],
            linkerSettings: [
                .linkedFramework("SafariServices"),
                .unsafeFlags([
                    "-Xlinker",
                    "-e",
                    "-Xlinker",
                    "_NSExtensionMain",
                ]),
            ]
        ),
        .executableTarget(
            name: "NonProxySafariStateProbe",
            dependencies: ["NonProxySafariStateBridge"]
        ),
        .testTarget(
            name: "NonProxyMacNetworkIdentityTests",
            dependencies: ["NonProxyMacNetworkIdentity"]
        ),
        .testTarget(
            name: "NonProxyProviderCoreTests",
            dependencies: [
                "NonProxyProviderCore",
                "NonProxyProviderContracts",
            ]
        ),
        .testTarget(
            name: "NonProxyMacPlatformSupportTests",
            dependencies: [
                "NonProxyMacNetworkIdentity",
                "NonProxyMacPlatformSupport",
                "NonProxyProviderContracts",
                "NonProxyProviderCore",
            ]
        ),
        .testTarget(
            name: "NonProxyTransparentProxyTests",
            dependencies: ["NonProxyTransparentProxy"]
        ),
        .testTarget(
            name: "NonProxyDNSProxyTests",
            dependencies: [
                "NonProxyDNSProxy",
                "NonProxyMacNetworkIdentity",
                "NonProxyMacPlatformSupport",
                "NonProxyProviderContracts",
                "NonProxyProviderCore",
            ]
        ),
        .testTarget(
            name: "NonProxyMacRuntimeTests",
            dependencies: ["NonProxyMacRuntime"]
        ),
        .testTarget(
            name: "NonProxyMacHostBridgeTests",
            dependencies: ["NonProxyMacHostBridge"]
        ),
        .testTarget(
            name: "NonProxyNativeMessagingTests",
            dependencies: ["NonProxyNativeMessaging"]
        ),
    ]
)
