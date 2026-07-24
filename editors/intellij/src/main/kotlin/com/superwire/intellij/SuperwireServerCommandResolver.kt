package com.superwire.intellij

import com.intellij.ide.plugins.PluginManagerCore
import com.intellij.openapi.application.PathManager
import com.intellij.openapi.diagnostic.Logger
import com.intellij.openapi.extensions.PluginId
import java.io.File
import java.io.InputStream
import java.nio.channels.Channels
import java.nio.file.AtomicMoveNotSupportedException
import java.nio.file.FileAlreadyExistsException
import java.nio.file.Files
import java.nio.file.InvalidPathException
import java.nio.file.LinkOption
import java.nio.file.OpenOption
import java.nio.file.Path
import java.nio.file.Paths
import java.nio.file.StandardCopyOption
import java.nio.file.StandardOpenOption
import java.nio.file.attribute.BasicFileAttributes
import java.nio.file.attribute.PosixFilePermission
import java.nio.file.attribute.PosixFilePermissions
import java.security.DigestInputStream
import java.security.MessageDigest
import java.util.HexFormat

object SuperwireServerCommandResolver {
    private const val PLUGIN_CACHE_DIRECTORY = "superwire-lsp"
    private const val DEVELOPMENT_PLUGIN_VERSION = "development"

    private val logger = Logger.getInstance(SuperwireServerCommandResolver::class.java)

    fun resolveServerCommand(): List<String> {
        val pluginSystemDirectory = Paths.get(PathManager.getSystemPath())
        val pluginCacheDirectory = pluginSystemDirectory.resolve(PLUGIN_CACHE_DIRECTORY)
        val pluginDescriptor = PluginManagerCore.getPlugin(PluginId.getId(SuperwirePluginConstants.PLUGIN_ID))
        val pluginVersion = pluginDescriptor?.version ?: DEVELOPMENT_PLUGIN_VERSION
        val runtimePlatform = SuperwireRuntimePlatform.current()

        val resolver = SuperwireServerBinaryResolver(
            environment = System.getenv(),
            pluginCacheDirectory = pluginCacheDirectory,
            pluginVersion = pluginVersion,
            runtimePlatform = runtimePlatform,
            bundledResourceLoader = SuperwireBundledResourceLoader { resourcePath ->
                javaClass.classLoader.getResourceAsStream(resourcePath)
            },
            logger = IntelliJServerResolutionLogger(logger),
        )
        return resolver.resolve().command()
    }
}

internal enum class SuperwireOperatingSystem(
    val resourceDirectoryName: String?,
    val pathSeparator: String,
) {
    Windows("windows", ";"),
    MacOs("macos", ":"),
    Linux("linux", ":"),
    Unsupported(null, File.pathSeparator),
    ;

    fun candidateBinaryFileNames(binaryName: String): List<String> {
        return if (this == Windows) {
            listOf("$binaryName.exe", binaryName)
        } else {
            listOf(binaryName, "$binaryName.exe")
        }
    }

    fun isRegularBinaryFile(binaryPath: Path): Boolean {
        val binaryAttributes = try {
            Files.readAttributes(binaryPath, BasicFileAttributes::class.java, LinkOption.NOFOLLOW_LINKS)
        } catch (_: Exception) {
            return false
        }

        return binaryAttributes.isRegularFile && !binaryAttributes.isSymbolicLink && binaryAttributes.size() > 0L
    }

    fun isExecutableBinary(binaryPath: Path): Boolean {
        if (!isRegularBinaryFile(binaryPath)) {
            return false
        }

        return this == Windows || Files.isExecutable(binaryPath)
    }

    fun ensurePrivateCacheDirectory(cacheDirectoryPath: Path) {
        try {
            if (this == Windows) {
                Files.createDirectory(cacheDirectoryPath)
            } else {
                Files.createDirectory(
                    cacheDirectoryPath,
                    PosixFilePermissions.asFileAttribute(PRIVATE_CACHE_DIRECTORY_PERMISSIONS),
                )
            }
        } catch (_: FileAlreadyExistsException) {
            // A concurrent resolver may have created the directory first.
        }

        val cacheDirectoryAttributes = Files.readAttributes(
            cacheDirectoryPath,
            BasicFileAttributes::class.java,
            LinkOption.NOFOLLOW_LINKS,
        )

        if (!cacheDirectoryAttributes.isDirectory || cacheDirectoryAttributes.isSymbolicLink) {
            throw IllegalStateException("Language server cache path is not a private directory: $cacheDirectoryPath")
        }

        if (this == Windows) {
            return
        }

        try {
            Files.setPosixFilePermissions(cacheDirectoryPath, PRIVATE_CACHE_DIRECTORY_PERMISSIONS)
        } catch (_: UnsupportedOperationException) {
            val cacheDirectoryFile = cacheDirectoryPath.toFile()
            val removedSharedRead = cacheDirectoryFile.setReadable(false, false)
            val removedSharedWrite = cacheDirectoryFile.setWritable(false, false)
            val removedSharedExecute = cacheDirectoryFile.setExecutable(false, false)
            val addedOwnerRead = cacheDirectoryFile.setReadable(true, true)
            val addedOwnerWrite = cacheDirectoryFile.setWritable(true, true)
            val addedOwnerExecute = cacheDirectoryFile.setExecutable(true, true)

            if (
                !removedSharedRead ||
                !removedSharedWrite ||
                !removedSharedExecute ||
                !addedOwnerRead ||
                !addedOwnerWrite ||
                !addedOwnerExecute
            ) {
                throw IllegalStateException("Unable to restrict language server cache directory permissions at $cacheDirectoryPath")
            }
        }
    }

    fun ensureBundledBinaryIsExecutable(binaryPath: Path) {
        if (!isRegularBinaryFile(binaryPath)) {
            throw IllegalStateException("Bundled language server is not a non-empty regular file at $binaryPath")
        }

        if (this == Windows) {
            return
        }

        try {
            val permissions = Files.getPosixFilePermissions(binaryPath, LinkOption.NOFOLLOW_LINKS)
            permissions.add(PosixFilePermission.OWNER_EXECUTE)
            Files.setPosixFilePermissions(binaryPath, permissions)
        } catch (_: UnsupportedOperationException) {
            if (!binaryPath.toFile().setExecutable(true, true)) {
                throw IllegalStateException("Unable to make bundled language server executable at $binaryPath")
            }
        }

        if (!isExecutableBinary(binaryPath)) {
            throw IllegalStateException("Bundled language server is not executable at $binaryPath")
        }
    }

    companion object {
        private val PRIVATE_CACHE_DIRECTORY_PERMISSIONS = PosixFilePermissions.fromString("rwx------")
        fun fromSystemName(operatingSystemName: String): SuperwireOperatingSystem {
            val normalizedOperatingSystemName = operatingSystemName.lowercase()

            return when {
                normalizedOperatingSystemName.contains("mac") || normalizedOperatingSystemName.contains("darwin") -> MacOs
                normalizedOperatingSystemName.contains("win") -> Windows
                normalizedOperatingSystemName.contains("linux") -> Linux
                else -> Unsupported
            }
        }
    }
}

internal enum class SuperwireArchitecture(val resourceDirectoryName: String?) {
    X86_64("x86_64"),
    AArch64("aarch64"),
    Unsupported(null),
    ;

    companion object {
        fun fromSystemName(architectureName: String): SuperwireArchitecture {
            return when (architectureName.lowercase()) {
                "x86_64", "amd64", "x64" -> X86_64
                "aarch64", "arm64" -> AArch64
                else -> Unsupported
            }
        }
    }
}

internal data class SuperwireRuntimePlatform(
    val operatingSystem: SuperwireOperatingSystem,
    val architecture: SuperwireArchitecture,
) {
    val resourceDirectory: String?
        get() {
            val operatingSystemDirectory = operatingSystem.resourceDirectoryName ?: return null
            val architectureDirectory = architecture.resourceDirectoryName ?: return null

            return "$operatingSystemDirectory-$architectureDirectory"
        }

    val packagedResourceDirectory: String?
        get() = when {
            operatingSystem == SuperwireOperatingSystem.Linux && architecture == SuperwireArchitecture.X86_64 -> resourceDirectory
            operatingSystem == SuperwireOperatingSystem.Windows && architecture == SuperwireArchitecture.X86_64 -> resourceDirectory
            operatingSystem == SuperwireOperatingSystem.MacOs &&
                (architecture == SuperwireArchitecture.X86_64 || architecture == SuperwireArchitecture.AArch64) -> resourceDirectory
            else -> null
        }

    fun candidateBinaryFileNames(): List<String> {
        return operatingSystem.candidateBinaryFileNames(SuperwirePluginConstants.SERVER_BINARY_NAME)
    }

    fun candidateBundledResourcePaths(): List<String> {
        val platformDirectory = packagedResourceDirectory ?: return emptyList()
        val binaryFileName = candidateBinaryFileNames().first()

        return listOf("lsp/bin/$platformDirectory/$binaryFileName")
    }

    companion object {
        fun current(): SuperwireRuntimePlatform {
            val operatingSystemName = System.getProperty("os.name").orEmpty()
            val architectureName = System.getProperty("os.arch").orEmpty()

            return SuperwireRuntimePlatform(
                operatingSystem = SuperwireOperatingSystem.fromSystemName(operatingSystemName),
                architecture = SuperwireArchitecture.fromSystemName(architectureName),
            )
        }
    }
}

internal enum class SuperwireServerResolutionSource(val description: String) {
    EnvironmentOverride("SUPERWIRE_LSP_PATH environment override"),
    BundledBinary("bundled plugin resource"),
    PathEnvironment("PATH environment"),
}

internal data class SuperwireResolvedServerBinary(
    val binaryPath: Path,
    val source: SuperwireServerResolutionSource,
) {
    fun command(): List<String> {
        return listOf(binaryPath.toString())
    }
}

internal fun interface SuperwireBundledResourceLoader {
    fun open(resourcePath: String): InputStream?
}

internal interface SuperwireServerResolutionLogger {
    fun debug(message: String)

    fun info(message: String)

    fun warning(message: String, throwable: Throwable? = null)
}

private class IntelliJServerResolutionLogger(private val logger: Logger) : SuperwireServerResolutionLogger {
    override fun debug(message: String) {
        logger.debug(message)
    }

    override fun info(message: String) {
        logger.info(message)
    }

    override fun warning(message: String, throwable: Throwable?) {
        if (throwable == null) {
            logger.warn(message)
        } else {
            logger.warn(message, throwable)
        }
    }
}

private class SuperwireBundledBinaryCacheCandidate(
    val binaryPath: Path,
    private val expectedContentHash: String,
    private val operatingSystem: SuperwireOperatingSystem,
) {
    fun isTrustedExecutable(): Boolean {
        if (!operatingSystem.isExecutableBinary(binaryPath)) {
            return false
        }

        return contentHash() == expectedContentHash
    }

    private fun contentHash(): String? {
        if (!operatingSystem.isRegularBinaryFile(binaryPath)) {
            return null
        }

        val messageDigest = MessageDigest.getInstance("SHA-256")

        return try {
            Files.newByteChannel(
                binaryPath,
                setOf<OpenOption>(StandardOpenOption.READ, LinkOption.NOFOLLOW_LINKS),
            ).use { binaryChannel ->
                DigestInputStream(Channels.newInputStream(binaryChannel), messageDigest).use { digestInputStream ->
                    val readBuffer = ByteArray(8192)

                    while (digestInputStream.read(readBuffer) != -1) {
                        // Reading the complete file updates the digest.
                    }
                }
            }

            HexFormat.of().formatHex(messageDigest.digest())
        } catch (_: Exception) {
            null
        }
    }
}

internal class SuperwireServerBinaryResolver(
    private val environment: Map<String, String>,
    private val pluginCacheDirectory: Path,
    pluginVersion: String,
    private val runtimePlatform: SuperwireRuntimePlatform,
    private val bundledResourceLoader: SuperwireBundledResourceLoader,
    private val logger: SuperwireServerResolutionLogger,
) {
    private val pluginVersionDirectoryName = pluginVersion.toSafeCacheDirectoryName()

    fun resolve(): SuperwireResolvedServerBinary {
        resolveEnvironmentOverride()?.let { environmentBinaryPath ->
            return resolvedBinary(environmentBinaryPath, SuperwireServerResolutionSource.EnvironmentOverride)
        }

        resolveBundledBinary()?.let { bundledBinaryPath ->
            return resolvedBinary(bundledBinaryPath, SuperwireServerResolutionSource.BundledBinary)
        }

        resolvePathEnvironment()?.let { pathEnvironmentBinaryPath ->
            return resolvedBinary(pathEnvironmentBinaryPath, SuperwireServerResolutionSource.PathEnvironment)
        }

        val platformName = runtimePlatform.resourceDirectory ?: "this runtime platform"
        val bundledResourceGuidance = runtimePlatform.packagedResourceDirectory?.let { packagedDirectory ->
            "reinstall the plugin artifact containing lsp/bin/$packagedDirectory/${runtimePlatform.candidateBinaryFileNames().first()}"
        } ?: "this plugin does not publish a bundled language server for $platformName"
        val failureMessage =
            "Unable to resolve ${SuperwirePluginConstants.SERVER_BINARY_NAME}: set $SERVER_PATH_ENVIRONMENT_VARIABLE to a trusted " +
                "executable, $bundledResourceGuidance, or place ${runtimePlatform.candidateBinaryFileNames().first()} on PATH. " +
                "Project-local target directories are never searched."

        logger.warning(failureMessage)
        throw IllegalStateException(failureMessage)
    }

    private fun resolveEnvironmentOverride(): Path? {
        val configuredServerPath = environmentValue(SERVER_PATH_ENVIRONMENT_VARIABLE)

        if (configuredServerPath.isNullOrBlank()) {
            return null
        }

        val normalizedServerPath = try {
            Paths.get(configuredServerPath).toAbsolutePath().normalize()
        } catch (invalidPathException: InvalidPathException) {
            logger.warning(
                "Ignoring invalid $SERVER_PATH_ENVIRONMENT_VARIABLE value '$configuredServerPath'",
                invalidPathException,
            )

            return null
        }

        if (!runtimePlatform.operatingSystem.isExecutableBinary(normalizedServerPath)) {
            logger.warning(
                "Ignoring $SERVER_PATH_ENVIRONMENT_VARIABLE because '$normalizedServerPath' is not a non-empty executable file",
            )

            return null
        }

        return normalizedServerPath
    }

    private fun resolveBundledBinary(): Path? {
        for (bundledResourcePath in runtimePlatform.candidateBundledResourcePaths()) {
            val bundledResourceStream = try {
                bundledResourceLoader.open(bundledResourcePath)
            } catch (resourceException: Exception) {
                logger.warning("Failed to open bundled language server resource '$bundledResourcePath'", resourceException)
                continue
            } ?: continue

            try {
                return extractBundledBinary(bundledResourcePath, bundledResourceStream)
            } catch (extractionException: Exception) {
                logger.warning("Failed to extract bundled language server resource '$bundledResourcePath'", extractionException)
            }
        }

        logger.debug("No usable bundled language server resource matched ${runtimePlatform.resourceDirectory ?: "the runtime platform"}")

        return null
    }

    private fun extractBundledBinary(bundledResourcePath: String, bundledResourceStream: InputStream): Path {
        return bundledResourceStream.use { resourceStream ->
            val resourceFileName = bundledResourcePath.substringAfterLast('/')
            val platformDirectoryName = runtimePlatform.resourceDirectory ?: "unsupported-platform"
            val versionCacheDirectory = pluginCacheDirectory.resolve(pluginVersionDirectoryName)
            val versionedPlatformDirectory = versionCacheDirectory.resolve(platformDirectoryName)

            runtimePlatform.operatingSystem.ensurePrivateCacheDirectory(pluginCacheDirectory)
            runtimePlatform.operatingSystem.ensurePrivateCacheDirectory(versionCacheDirectory)
            runtimePlatform.operatingSystem.ensurePrivateCacheDirectory(versionedPlatformDirectory)

            val temporaryBinaryPath = Files.createTempFile(
                versionedPlatformDirectory,
                ".$resourceFileName-",
                ".tmp",
            )

            try {
                val messageDigest = MessageDigest.getInstance("SHA-256")

                DigestInputStream(resourceStream, messageDigest).use { digestInputStream ->
                    Files.copy(digestInputStream, temporaryBinaryPath, StandardCopyOption.REPLACE_EXISTING)
                }

                if (!runtimePlatform.operatingSystem.isRegularBinaryFile(temporaryBinaryPath)) {
                    throw IllegalStateException("Bundled language server resource '$bundledResourcePath' is empty or not regular")
                }

                runtimePlatform.operatingSystem.ensureBundledBinaryIsExecutable(temporaryBinaryPath)

                val resourceHash = HexFormat.of().formatHex(messageDigest.digest())
                val hashedCacheDirectory = versionedPlatformDirectory.resolve(resourceHash)
                val extractedBinaryPath = hashedCacheDirectory.resolve(resourceFileName)

                runtimePlatform.operatingSystem.ensurePrivateCacheDirectory(hashedCacheDirectory)

                val cacheCandidate = SuperwireBundledBinaryCacheCandidate(
                    binaryPath = extractedBinaryPath,
                    expectedContentHash = resourceHash,
                    operatingSystem = runtimePlatform.operatingSystem,
                )

                if (cacheCandidate.isTrustedExecutable()) {
                    return@use extractedBinaryPath
                }

                if (Files.exists(extractedBinaryPath, LinkOption.NOFOLLOW_LINKS)) {
                    logger.warning("Replacing untrusted bundled language server cache entry at $extractedBinaryPath")
                }

                installAtomically(temporaryBinaryPath, extractedBinaryPath)
                runtimePlatform.operatingSystem.ensureBundledBinaryIsExecutable(extractedBinaryPath)

                if (!cacheCandidate.isTrustedExecutable()) {
                    throw IllegalStateException("Extracted language server failed integrity verification at $extractedBinaryPath")
                }

                extractedBinaryPath
            } finally {
                Files.deleteIfExists(temporaryBinaryPath)
            }
        }
    }

    private fun installAtomically(temporaryBinaryPath: Path, extractedBinaryPath: Path) {
        try {
            Files.move(
                temporaryBinaryPath,
                extractedBinaryPath,
                StandardCopyOption.ATOMIC_MOVE,
                StandardCopyOption.REPLACE_EXISTING,
            )
        } catch (atomicMoveException: AtomicMoveNotSupportedException) {
            throw IllegalStateException(
                "The language server cache filesystem does not support atomic replacement at $extractedBinaryPath",
                atomicMoveException,
            )
        }
    }

    private fun resolvePathEnvironment(): Path? {
        val pathEnvironmentValue = environmentValue("PATH")

        if (pathEnvironmentValue.isNullOrBlank()) {
            return null
        }

        val pathDirectories = pathEnvironmentValue
            .split(runtimePlatform.operatingSystem.pathSeparator)
            .filter(String::isNotBlank)

        for (pathDirectoryText in pathDirectories) {
            val pathDirectory = try {
                Paths.get(pathDirectoryText)
            } catch (invalidPathException: InvalidPathException) {
                logger.debug("Ignoring invalid PATH entry '$pathDirectoryText': ${invalidPathException.message}")
                continue
            }

            for (binaryFileName in runtimePlatform.candidateBinaryFileNames()) {
                val candidateBinaryPath = pathDirectory.resolve(binaryFileName).toAbsolutePath().normalize()

                if (runtimePlatform.operatingSystem.isExecutableBinary(candidateBinaryPath)) {
                    return candidateBinaryPath
                }
            }
        }

        return null
    }

    private fun resolvedBinary(
        binaryPath: Path,
        resolutionSource: SuperwireServerResolutionSource,
    ): SuperwireResolvedServerBinary {
        val resolvedBinary = SuperwireResolvedServerBinary(binaryPath, resolutionSource)

        logger.info("Resolved Superwire language server from ${resolutionSource.description}: ${resolvedBinary.binaryPath}")

        return resolvedBinary
    }

    private fun environmentValue(variableName: String): String? {
        if (runtimePlatform.operatingSystem != SuperwireOperatingSystem.Windows) {
            return environment[variableName]
        }

        return environment.entries
            .firstOrNull { environmentEntry -> environmentEntry.key.equals(variableName, ignoreCase = true) }
            ?.value
    }

    private fun String.toSafeCacheDirectoryName(): String {
        if (isBlank()) {
            return "development"
        }

        return map { character ->
            if (character.isLetterOrDigit() || character == '.' || character == '-' || character == '_') {
                character
            } else {
                '_'
            }
        }.joinToString(separator = "")
    }

    private companion object {
        const val SERVER_PATH_ENVIRONMENT_VARIABLE = "SUPERWIRE_LSP_PATH"
    }
}
