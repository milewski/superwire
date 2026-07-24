package com.superwire.intellij

import java.io.ByteArrayInputStream
import java.nio.file.Files
import java.nio.file.LinkOption
import java.nio.file.Path
import java.nio.file.attribute.PosixFilePermission
import java.nio.file.attribute.PosixFilePermissions
import java.security.MessageDigest
import java.util.HexFormat
import java.util.concurrent.CountDownLatch
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertFailsWith
import kotlin.test.assertNotEquals
import kotlin.test.assertTrue
import org.junit.jupiter.api.condition.EnabledOnOs
import org.junit.jupiter.api.condition.OS
import org.junit.jupiter.api.io.TempDir

class SuperwireBundledServerBinaryTest {
    @TempDir
    lateinit var temporaryDirectory: Path

    private val packagedHostRuntimePlatform = SuperwireRuntimePlatform.current()
    private val hostFilesystem = SuperwireHostFilesystem.current()

    @Test
    fun `scopes extracted binaries by plugin version platform and content hash`() {
        val firstBinaryContents = executableContents("first")
        val secondBinaryContents = executableContents("second")
        val firstResolution = createBundledResolver(firstBinaryContents).resolve()
        val secondResolution = createBundledResolver(secondBinaryContents).resolve()
        val expectedFirstPath = expectedCachePath(firstBinaryContents)
        val expectedSecondPath = expectedCachePath(secondBinaryContents)

        assertEquals(expectedFirstPath, firstResolution.binaryPath)
        assertEquals(expectedSecondPath, secondResolution.binaryPath)
        assertNotEquals(firstResolution.binaryPath, secondResolution.binaryPath)
        assertTrue(hostFilesystem.isExecutableBinary(firstResolution.binaryPath))
        assertTrue(hostFilesystem.isExecutableBinary(secondResolution.binaryPath))
        assertEquals(firstBinaryContents.toList(), Files.readAllBytes(firstResolution.binaryPath).toList())
        assertEquals(secondBinaryContents.toList(), Files.readAllBytes(secondResolution.binaryPath).toList())

        if (
            hostFilesystem == SuperwireHostFilesystem.Posix &&
            Files.getFileStore(firstResolution.binaryPath).supportsFileAttributeView("posix")
        ) {
            val privateDirectoryPermissions = PosixFilePermissions.fromString("rwx------")
            val cacheDirectory = temporaryDirectory.resolve("cache")
            val versionDirectory = cacheDirectory.resolve("1.2.3")
            val platformDirectory = versionDirectory.resolve(requireNotNull(packagedHostRuntimePlatform.packagedResourceDirectory))
            val hashDirectory = firstResolution.binaryPath.parent
            val binaryPermissions = Files.getPosixFilePermissions(firstResolution.binaryPath, LinkOption.NOFOLLOW_LINKS)

            assertEquals(privateDirectoryPermissions, Files.getPosixFilePermissions(cacheDirectory, LinkOption.NOFOLLOW_LINKS))
            assertEquals(privateDirectoryPermissions, Files.getPosixFilePermissions(versionDirectory, LinkOption.NOFOLLOW_LINKS))
            assertEquals(privateDirectoryPermissions, Files.getPosixFilePermissions(platformDirectory, LinkOption.NOFOLLOW_LINKS))
            assertEquals(privateDirectoryPermissions, Files.getPosixFilePermissions(hashDirectory, LinkOption.NOFOLLOW_LINKS))
            assertTrue(binaryPermissions.contains(PosixFilePermission.OWNER_EXECUTE))
        }
    }

    @Test
    fun `replaces a wrong executable in the expected hash directory`() {
        val bundledBinaryContents = executableContents("trusted")
        val wrongBinaryContents = executableContents("wrong")
        val expectedBinaryPath = expectedCachePath(bundledBinaryContents)

        Files.createDirectories(expectedBinaryPath.parent)
        Files.write(expectedBinaryPath, wrongBinaryContents)
        hostFilesystem.ensureBundledBinaryIsExecutable(expectedBinaryPath)

        val resolution = createBundledResolver(bundledBinaryContents).resolve()

        assertEquals(expectedBinaryPath, resolution.binaryPath)
        assertEquals(bundledBinaryContents.toList(), Files.readAllBytes(resolution.binaryPath).toList())
        assertTrue(hostFilesystem.isExecutableBinary(resolution.binaryPath))
    }

    @Test
    fun `Windows resource extraction preserves exe naming and rejects a wrong cached hash`() {
        val bundledBinaryContents = executableContents("windows-trusted")
        val wrongBinaryContents = executableContents("windows-wrong")
        val requestedBundledResourcePaths = mutableListOf<String>()
        val windowsRuntimePlatform = SuperwireRuntimePlatform(
            SuperwireOperatingSystem.Windows,
            SuperwireArchitecture.X86_64,
        )
        val resolver = createBundledResolver(
            binaryContents = bundledBinaryContents,
            cacheDirectoryName = "windows-cache",
            runtimePlatform = windowsRuntimePlatform,
            filesystem = SuperwireHostFilesystem.Windows,
            requestedBundledResourcePaths = requestedBundledResourcePaths,
        )
        val expectedBinaryPath = expectedCachePath(
            binaryContents = bundledBinaryContents,
            cacheDirectoryName = "windows-cache",
            runtimePlatform = windowsRuntimePlatform,
        )

        val firstResolution = resolver.resolve()

        assertEquals(expectedBinaryPath, firstResolution.binaryPath)
        assertEquals(listOf("lsp/bin/windows-x86_64/superwire-lsp.exe"), requestedBundledResourcePaths)
        assertTrue(SuperwireHostFilesystem.Windows.isExecutableBinary(firstResolution.binaryPath))

        Files.write(expectedBinaryPath, wrongBinaryContents)

        val secondResolution = resolver.resolve()

        assertEquals(expectedBinaryPath, secondResolution.binaryPath)
        assertEquals(bundledBinaryContents.toList(), Files.readAllBytes(secondResolution.binaryPath).toList())
        assertEquals(
            listOf(
                "lsp/bin/windows-x86_64/superwire-lsp.exe",
                "lsp/bin/windows-x86_64/superwire-lsp.exe",
            ),
            requestedBundledResourcePaths,
        )
    }

    @Test
    fun `rejects a nonregular cache entry`() {
        val bundledBinaryContents = executableContents("trusted")
        val expectedBinaryPath = expectedCachePath(bundledBinaryContents)

        Files.createDirectories(expectedBinaryPath)

        assertFailsWith<IllegalStateException> {
            createBundledResolver(bundledBinaryContents).resolve()
        }

        assertTrue(Files.isDirectory(expectedBinaryPath, LinkOption.NOFOLLOW_LINKS))
        assertFalse(Files.isSymbolicLink(expectedBinaryPath))
    }

    @Test
    @EnabledOnOs(OS.LINUX, OS.MAC)
    fun `replaces a malicious symlink without modifying its target`() {
        val bundledBinaryContents = executableContents("trusted")
        val maliciousTargetContents = executableContents("malicious")
        val maliciousTargetPath = temporaryDirectory.resolve("malicious-server")
        val expectedBinaryPath = expectedCachePath(bundledBinaryContents)

        Files.write(maliciousTargetPath, maliciousTargetContents)
        hostFilesystem.ensureBundledBinaryIsExecutable(maliciousTargetPath)
        Files.createDirectories(expectedBinaryPath.parent)
        Files.createSymbolicLink(expectedBinaryPath, maliciousTargetPath)

        val resolution = createBundledResolver(bundledBinaryContents).resolve()

        assertEquals(expectedBinaryPath, resolution.binaryPath)
        assertFalse(Files.isSymbolicLink(resolution.binaryPath))
        assertEquals(bundledBinaryContents.toList(), Files.readAllBytes(resolution.binaryPath).toList())
        assertEquals(maliciousTargetContents.toList(), Files.readAllBytes(maliciousTargetPath).toList())
        assertTrue(hostFilesystem.isExecutableBinary(resolution.binaryPath))
    }

    @Test
    fun `concurrent resolution installs one complete executable`() {
        val bundledBinaryContents = executableContents("concurrent")
        val resolver = createBundledResolver(bundledBinaryContents)
        val workerCount = 12
        val startSignal = CountDownLatch(1)
        val workerPool = Executors.newFixedThreadPool(workerCount)

        try {
            val resolutionFutures = (1..workerCount).map {
                workerPool.submit<Path> {
                    startSignal.await()
                    resolver.resolve().binaryPath
                }
            }

            startSignal.countDown()

            val resolvedPaths = resolutionFutures.map { resolutionFuture ->
                resolutionFuture.get(30, TimeUnit.SECONDS)
            }

            assertEquals(1, resolvedPaths.toSet().size)

            val resolvedBinaryPath = resolvedPaths.toSet().single()

            assertEquals(expectedCachePath(bundledBinaryContents), resolvedBinaryPath)
            assertEquals(bundledBinaryContents.toList(), Files.readAllBytes(resolvedBinaryPath).toList())
            assertTrue(hostFilesystem.isExecutableBinary(resolvedBinaryPath))
        } finally {
            workerPool.shutdownNow()
        }
    }

    private fun createBundledResolver(
        binaryContents: ByteArray,
        cacheDirectoryName: String = "cache",
        runtimePlatform: SuperwireRuntimePlatform = packagedHostRuntimePlatform,
        filesystem: SuperwireHostFilesystem = hostFilesystem,
        requestedBundledResourcePaths: MutableList<String>? = null,
    ): SuperwireServerBinaryResolver {
        val bundledResourcePath = runtimePlatform.candidateBundledResourcePaths().single()

        return SuperwireServerBinaryResolver(
            environment = emptyMap(),
            pluginCacheDirectory = temporaryDirectory.resolve(cacheDirectoryName),
            pluginVersion = "1.2.3",
            runtimePlatform = runtimePlatform,
            hostFilesystem = filesystem,
            bundledResourceLoader = SuperwireBundledResourceLoader { resourcePath ->
                requestedBundledResourcePaths?.add(resourcePath)

                if (resourcePath == bundledResourcePath) {
                    ByteArrayInputStream(binaryContents)
                } else {
                    null
                }
            },
            logger = SilentServerResolutionLogger,
        )
    }

    private fun expectedCachePath(
        binaryContents: ByteArray,
        cacheDirectoryName: String = "cache",
        runtimePlatform: SuperwireRuntimePlatform = packagedHostRuntimePlatform,
    ): Path {
        val contentHash = HexFormat.of().formatHex(MessageDigest.getInstance("SHA-256").digest(binaryContents))
        val packagedResourceDirectory = requireNotNull(runtimePlatform.packagedResourceDirectory)
        val binaryFileName = runtimePlatform.candidateBinaryFileNames().first()

        return temporaryDirectory
            .resolve(cacheDirectoryName)
            .resolve("1.2.3")
            .resolve(packagedResourceDirectory)
            .resolve(contentHash)
            .resolve(binaryFileName)
    }

    private fun executableContents(marker: String): ByteArray {
        return "#!/bin/sh\n# $marker\nexit 0\n".toByteArray()
    }

    private object SilentServerResolutionLogger : SuperwireServerResolutionLogger {
        override fun debug(message: String) = Unit

        override fun info(message: String) = Unit

        override fun warning(message: String, throwable: Throwable?) = Unit
    }
}
