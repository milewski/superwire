package com.superwire.intellij

import com.intellij.openapi.application.PathManager
import com.intellij.openapi.project.Project
import com.intellij.openapi.util.SystemInfo
import java.nio.file.Files
import java.nio.file.Path
import java.nio.file.Paths
import java.nio.file.StandardCopyOption

object SuperwireServerCommandResolver {
    private const val SERVER_PATH_ENVIRONMENT_VARIABLE = "SUPERWIRE_LSP_PATH"
    private const val PLUGIN_CACHE_DIRECTORY = "superwire-lsp"

    fun resolveServerCommand(project: Project): List<String> {
        val environmentServerPath = resolveEnvironmentServerPath()

        if (environmentServerPath != null) {
            return listOf(environmentServerPath.toString())
        }

        val bundledServerPath = extractBundledServerBinary()

        if (bundledServerPath != null) {
            return listOf(bundledServerPath.toString())
        }

        val projectServerPath = findProjectServerBinary(project)

        if (projectServerPath != null) {
            return listOf(projectServerPath.toString())
        }

        return listOf(defaultServerCommandName())
    }

    private fun resolveEnvironmentServerPath(): Path? {
        val configuredServerPath = System.getenv(SERVER_PATH_ENVIRONMENT_VARIABLE)

        if (configuredServerPath.isNullOrBlank()) {
            return null
        }

        val configuredPath = Paths.get(configuredServerPath)

        if (!isExecutableServerBinary(configuredPath)) {
            return null
        }

        return configuredPath
    }

    private fun extractBundledServerBinary(): Path? {
        val pluginSystemDirectory = Paths.get(PathManager.getSystemPath())
        val pluginCacheDirectory = pluginSystemDirectory.resolve(PLUGIN_CACHE_DIRECTORY)

        Files.createDirectories(pluginCacheDirectory)

        for (bundledBinaryResourcePath in candidateBundledResourcePaths()) {
            val bundledBinaryFileName = Paths.get(bundledBinaryResourcePath).fileName.toString()
            val bundledBinaryStream = javaClass.classLoader.getResourceAsStream(bundledBinaryResourcePath) ?: continue

            val extractedBinaryPath = pluginCacheDirectory
                .resolve(runtimePlatformDirectory())
                .resolve(bundledBinaryFileName)

            Files.createDirectories(extractedBinaryPath.parent)

            bundledBinaryStream.use { inputStream ->
                Files.copy(inputStream, extractedBinaryPath, StandardCopyOption.REPLACE_EXISTING)
            }

            if (!SystemInfo.isWindows) {
                extractedBinaryPath.toFile().setExecutable(true)
            }

            if (isExecutableServerBinary(extractedBinaryPath)) {
                return extractedBinaryPath
            }
        }

        return null
    }

    private fun findProjectServerBinary(project: Project): Path? {
        val projectBasePath = project.basePath ?: return null
        val projectRootPath = Paths.get(projectBasePath)

        val candidateBinaryPaths = candidateBinaryFileNames().flatMap { bundledBinaryFileName ->
            listOf(
                projectRootPath.resolve("target/release").resolve(bundledBinaryFileName),
                projectRootPath.resolve("target/debug").resolve(bundledBinaryFileName),
                projectRootPath.resolve("../target/release").resolve(bundledBinaryFileName).normalize(),
                projectRootPath.resolve("../target/debug").resolve(bundledBinaryFileName).normalize(),
                projectRootPath.resolve("../../target/release").resolve(bundledBinaryFileName).normalize(),
                projectRootPath.resolve("../../target/debug").resolve(bundledBinaryFileName).normalize(),
            )
        }

        return candidateBinaryPaths.firstOrNull(::isExecutableServerBinary)
    }

    private fun candidateBinaryFileNames(): List<String> {
        if (SystemInfo.isWindows) {
            return listOf("${SuperwirePluginConstants.SERVER_BINARY_NAME}.exe", SuperwirePluginConstants.SERVER_BINARY_NAME)
        }

        return listOf(SuperwirePluginConstants.SERVER_BINARY_NAME, "${SuperwirePluginConstants.SERVER_BINARY_NAME}.exe")
    }

    private fun defaultServerCommandName(): String {
        if (SystemInfo.isWindows) {
            return "${SuperwirePluginConstants.SERVER_BINARY_NAME}.exe"
        }

        return SuperwirePluginConstants.SERVER_BINARY_NAME
    }

    private fun candidateBundledResourcePaths(): List<String> {
        val modernResourcePaths = candidateBinaryFileNames().map { binaryFileName ->
            "lsp/bin/${runtimePlatformDirectory()}/$binaryFileName"
        }

        val legacyResourcePaths = candidateBinaryFileNames().map { binaryFileName ->
            "lsp/bin/$binaryFileName"
        }

        return modernResourcePaths + legacyResourcePaths
    }

    private fun runtimePlatformDirectory(): String {
        return "${runtimeOperatingSystemName()}-${runtimeArchitectureName()}"
    }

    private fun runtimeOperatingSystemName(): String {
        if (SystemInfo.isWindows) {
            return "windows"
        }

        if (SystemInfo.isMac) {
            return "macos"
        }

        return "linux"
    }

    private fun runtimeArchitectureName(): String {
        val architectureName = System.getProperty("os.arch") ?: return "x86_64"
        val normalizedArchitectureName = architectureName.lowercase()

        if (normalizedArchitectureName == "x86_64" || normalizedArchitectureName == "amd64") {
            return "x86_64"
        }

        if (normalizedArchitectureName == "aarch64" || normalizedArchitectureName == "arm64") {
            return "aarch64"
        }

        return "x86_64"
    }

    private fun isExecutableServerBinary(binaryPath: Path): Boolean {
        if (!Files.exists(binaryPath)) {
            return false
        }

        if (SystemInfo.isWindows) {
            return true
        }

        return Files.isExecutable(binaryPath)
    }
}
