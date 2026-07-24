package com.superwire.intellij

import java.io.ByteArrayInputStream
import java.nio.file.Files
import java.nio.file.LinkOption
import java.nio.file.Path
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
        assertTrue(Files.isExecutable(firstResolution.binaryPath))
        assertTrue(Files.isExecutable(secondResolution.binaryPath))
        assertEquals(firstBinaryContents.toList(), Files.readAllBytes(firstResolution.binaryPath).toList())
        assertEquals(secondBinaryContents.toList(), Files.readAllBytes(secondResolution.binaryPath).toList())

        if (Files.getFileStore(firstResolution.binaryPath).supportsFileAttributeView("posix")) {
            val privateDirectoryPermissions = PosixFilePermissions.fromString("rwx------")
            val cacheDirectory = temporaryDirectory.resolve("cache")
            val versionDirectory = cacheDirectory.resolve("1.2.3")
            val platformDirectory = versionDirectory.resolve("linux-x86_64")
            val hashDirectory = firstResolution.binaryPath.parent

            assertEquals(privateDirectoryPermissions, Files.getPosixFilePermissions(cacheDirectory, LinkOption.NOFOLLOW_LINKS))
            assertEquals(privateDirectoryPermissions, Files.getPosixFilePermissions(versionDirectory, LinkOption.NOFOLLOW_LINKS))
            assertEquals(privateDirectoryPermissions, Files.getPosixFilePermissions(platformDirectory, LinkOption.NOFOLLOW_LINKS))
            assertEquals(privateDirectoryPermissions, Files.getPosixFilePermissions(hashDirectory, LinkOption.NOFOLLOW_LINKS))
        }
    }

    @Test
    fun `replaces a wrong executable in the expected hash directory`() {
        val bundledBinaryContents = executableContents("trusted")
        val wrongBinaryContents = executableContents("wrong")
        val expectedBinaryPath = expectedCachePath(bundledBinaryContents)

        Files.createDirectories(expectedBinaryPath.parent)
        Files.write(expectedBinaryPath, wrongBinaryContents)
        SuperwireOperatingSystem.Linux.ensureBundledBinaryIsExecutable(expectedBinaryPath)

        val resolution = createBundledResolver(bundledBinaryContents).resolve()

        assertEquals(expectedBinaryPath, resolution.binaryPath)
        assertEquals(bundledBinaryContents.toList(), Files.readAllBytes(resolution.binaryPath).toList())
        assertTrue(Files.isExecutable(resolution.binaryPath))
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
        SuperwireOperatingSystem.Linux.ensureBundledBinaryIsExecutable(maliciousTargetPath)
        Files.createDirectories(expectedBinaryPath.parent)
        Files.createSymbolicLink(expectedBinaryPath, maliciousTargetPath)

        val resolution = createBundledResolver(bundledBinaryContents).resolve()

        assertEquals(expectedBinaryPath, resolution.binaryPath)
        assertFalse(Files.isSymbolicLink(resolution.binaryPath))
        assertEquals(bundledBinaryContents.toList(), Files.readAllBytes(resolution.binaryPath).toList())
        assertEquals(maliciousTargetContents.toList(), Files.readAllBytes(maliciousTargetPath).toList())
        assertTrue(Files.isExecutable(resolution.binaryPath))
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
            assertTrue(Files.isExecutable(resolvedBinaryPath))
        } finally {
            workerPool.shutdownNow()
        }
    }

    private fun createBundledResolver(binaryContents: ByteArray): SuperwireServerBinaryResolver {
        return SuperwireServerBinaryResolver(
            environment = emptyMap(),
            pluginCacheDirectory = temporaryDirectory.resolve("cache"),
            pluginVersion = "1.2.3",
            runtimePlatform = SuperwireRuntimePlatform(
                SuperwireOperatingSystem.Linux,
                SuperwireArchitecture.X86_64,
            ),
            bundledResourceLoader = SuperwireBundledResourceLoader { resourcePath ->
                if (resourcePath == "lsp/bin/linux-x86_64/superwire-lsp") {
                    ByteArrayInputStream(binaryContents)
                } else {
                    null
                }
            },
            logger = SilentServerResolutionLogger,
        )
    }

    private fun expectedCachePath(binaryContents: ByteArray): Path {
        val contentHash = HexFormat.of().formatHex(MessageDigest.getInstance("SHA-256").digest(binaryContents))

        return temporaryDirectory
            .resolve("cache")
            .resolve("1.2.3")
            .resolve("linux-x86_64")
            .resolve(contentHash)
            .resolve("superwire-lsp")
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
