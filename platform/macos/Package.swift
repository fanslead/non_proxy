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
            name: "NonProxyTransparentProxy",
            targets: ["NonProxyTransparentProxy"]
        ),
        .executable(
            name: "NonProxyProviderSmoke",
            targets: ["NonProxyProviderSmoke"]
        ),
        .executable(
            name: "NonProxyFlowSmoke",
            targets: ["NonProxyFlowSmoke"]
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
            name: "NonProxyMacPlatformSupport",
            dependencies: [
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
                "NonProxyMacPlatformSupport",
                "NonProxyProviderCore",
                "NonProxyProviderContracts",
            ],
            linkerSettings: [
                .linkedFramework("Network"),
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
                "NonProxyMacPlatformSupport",
                "NonProxyProviderContracts",
            ]
        ),
        .testTarget(
            name: "NonProxyTransparentProxyTests",
            dependencies: ["NonProxyTransparentProxy"]
        ),
    ]
)
