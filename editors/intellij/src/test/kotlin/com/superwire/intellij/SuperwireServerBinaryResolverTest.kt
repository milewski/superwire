package com.superwire.intellij

import java.io.ByteArrayInputStream
import java.nio.file.Files
import java.nio.file.Path
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertFailsWith
import kotlin.test.assertTrue
import org.junit.jupiter.api.io.TempDir

class SuperwireServerBinaryResolverTest {
    @TempDir
    lateinit var temporaryDirectory: Path

    @Test
    fun `resolves only approved sources in deterministic precedence order`() {
        val environmentBinaryPath = createExecutableBinary(temporaryDirectory.resolve("environment/superwire-lsp"))
        val projectBinaryPath = createExecutableBinary(temporaryDirectory.resolve("project/target/release/superwire-lsp"))
        val pathDirectory = temporaryDirectory.resolve("path")
        val pathBinaryPath = createExecutableBinary(pathDirectory.resolve("superwire-lsp"))
        val bundledBinaryBytes = executableContents("bundled")

        val environmentResolution = createResolver(
            environment = mapOf(
                "SUPERWIRE_LSP_PATH" to environmentBinaryPath.toString(),
                "PATH" to pathDirectory.toString(),
            ),
            bundledBinaryBytes = bundledBinaryBytes,
            cacheDirectoryName = "environment-cache",
        ).resolve()

        assertEquals(SuperwireServerResolutionSource.EnvironmentOverride, environmentResolution.source)
        assertEquals(environmentBinaryPath.toAbsolutePath().normalize(), environmentResolution.binaryPath)

        val bundledResolution = createResolver(
            environment = mapOf("PATH" to pathDirectory.toString()),
            bundledBinaryBytes = bundledBinaryBytes,
            cacheDirectoryName = "bundled-cache",
        ).resolve()

        assertEquals(SuperwireServerResolutionSource.BundledBinary, bundledResolution.source)

        val pathResolution = createResolver(
            environment = mapOf("PATH" to pathDirectory.toString()),
            bundledBinaryBytes = null,
            cacheDirectoryName = "path-cache",
        ).resolve()

        assertEquals(SuperwireServerResolutionSource.PathEnvironment, pathResolution.source)
        assertEquals(pathBinaryPath.toAbsolutePath().normalize(), pathResolution.binaryPath)
        assertTrue(Files.isExecutable(projectBinaryPath))
        assertFalse(pathResolution.binaryPath.startsWith(projectBinaryPath.parent.parent.parent))
    }

    @Test
    fun `ignores an invalid environment override and logs why`() {
        val resolutionLogger = RecordingServerResolutionLogger()
        val missingEnvironmentBinaryPath = temporaryDirectory.resolve("missing/superwire-lsp")
        val resolver = createResolver(
            environment = mapOf("SUPERWIRE_LSP_PATH" to missingEnvironmentBinaryPath.toString()),
            bundledBinaryBytes = executableContents("bundled"),
            cacheDirectoryName = "invalid-environment-cache",
            resolutionLogger = resolutionLogger,
        )

        val resolution = resolver.resolve()

        assertEquals(SuperwireServerResolutionSource.BundledBinary, resolution.source)
        assertTrue(
            resolutionLogger.warningMessages.any { warningMessage ->
                warningMessage.contains("SUPERWIRE_LSP_PATH") && warningMessage.contains("not a non-empty executable file")
            },
        )
        assertTrue(
            resolutionLogger.infoMessages.single().contains(SuperwireServerResolutionSource.BundledBinary.description),
        )
    }

    @Test
    fun `unsupported packaged architecture uses PATH without probing bundled resources`() {
        val pathDirectory = temporaryDirectory.resolve("unsupported-platform-path")
        val pathBinaryPath = createExecutableBinary(pathDirectory.resolve("superwire-lsp"))
        val requestedBundledResourcePaths = mutableListOf<String>()
        val unsupportedPackagedPlatform = SuperwireRuntimePlatform(
            SuperwireOperatingSystem.Linux,
            SuperwireArchitecture.AArch64,
        )
        val resolver = createResolver(
            environment = mapOf("PATH" to pathDirectory.toString()),
            bundledBinaryBytes = executableContents("unreachable-bundle"),
            cacheDirectoryName = "unsupported-platform-cache",
            runtimePlatform = unsupportedPackagedPlatform,
            requestedBundledResourcePaths = requestedBundledResourcePaths,
        )

        val resolution = resolver.resolve()

        assertEquals(SuperwireServerResolutionSource.PathEnvironment, resolution.source)
        assertEquals(pathBinaryPath, resolution.binaryPath)
        assertEquals(emptyList(), requestedBundledResourcePaths)
    }

    @Test
    fun `reports every searched source when resolution fails`() {
        val resolutionLogger = RecordingServerResolutionLogger()
        val resolver = createResolver(
            environment = emptyMap(),
            bundledBinaryBytes = null,
            cacheDirectoryName = "failure-cache",
            resolutionLogger = resolutionLogger,
        )

        val resolutionFailure = assertFailsWith<IllegalStateException> {
            resolver.resolve()
        }

        val failureMessage = resolutionFailure.message.orEmpty()

        assertTrue(failureMessage.contains("set SUPERWIRE_LSP_PATH"))
        assertTrue(failureMessage.contains("reinstall the plugin artifact containing lsp/bin/linux-x86_64/superwire-lsp"))
        assertTrue(failureMessage.contains("place superwire-lsp on PATH"))
        assertTrue(failureMessage.contains("Project-local target directories are never searched"))
        assertFalse(failureMessage.contains("target/release"))
        assertEquals(resolutionFailure.message, resolutionLogger.warningMessages.last())
    }

    private fun createResolver(
        environment: Map<String, String>,
        bundledBinaryBytes: ByteArray?,
        cacheDirectoryName: String,
        runtimePlatform: SuperwireRuntimePlatform = linuxRuntimePlatform(),
        requestedBundledResourcePaths: MutableList<String>? = null,
        resolutionLogger: SuperwireServerResolutionLogger = RecordingServerResolutionLogger(),
    ): SuperwireServerBinaryResolver {
        return SuperwireServerBinaryResolver(
            environment = environment,
            pluginCacheDirectory = temporaryDirectory.resolve(cacheDirectoryName),
            pluginVersion = "test-version",
            runtimePlatform = runtimePlatform,
            bundledResourceLoader = SuperwireBundledResourceLoader { resourcePath ->
                requestedBundledResourcePaths?.add(resourcePath)

                if (bundledBinaryBytes != null && resourcePath == "lsp/bin/linux-x86_64/superwire-lsp") {
                    ByteArrayInputStream(bundledBinaryBytes)
                } else {
                    null
                }
            },
            logger = resolutionLogger,
        )
    }

    private fun createExecutableBinary(binaryPath: Path): Path {
        Files.createDirectories(binaryPath.parent)
        Files.write(binaryPath, executableContents(binaryPath.fileName.toString()))
        SuperwireOperatingSystem.Linux.ensureBundledBinaryIsExecutable(binaryPath)

        return binaryPath.toAbsolutePath().normalize()
    }

    private fun executableContents(marker: String): ByteArray {
        return "#!/bin/sh\n# $marker\nexit 0\n".toByteArray()
    }

    private fun linuxRuntimePlatform(): SuperwireRuntimePlatform {
        return SuperwireRuntimePlatform(SuperwireOperatingSystem.Linux, SuperwireArchitecture.X86_64)
    }

    private class RecordingServerResolutionLogger : SuperwireServerResolutionLogger {
        val infoMessages = mutableListOf<String>()
        val warningMessages = mutableListOf<String>()

        override fun debug(message: String) = Unit

        override fun info(message: String) {
            infoMessages.add(message)
        }

        override fun warning(message: String, throwable: Throwable?) {
            warningMessages.add(message)
        }
    }
}
