package com.engineai.intellij

import com.intellij.openapi.application.PathManager
import com.intellij.openapi.project.Project
import com.intellij.openapi.util.SystemInfo
import java.nio.file.Files
import java.nio.file.Path
import java.nio.file.Paths
import java.nio.file.StandardCopyOption

object EngineAiServerCommandResolver {
    private const val SERVER_PATH_ENVIRONMENT_VARIABLE = "ENGINE_AI_LSP_PATH"
    private const val PLUGIN_CACHE_DIRECTORY = "engine-ai-lsp"

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

        return listOf(EngineAiPluginConstants.SERVER_BINARY_NAME)
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
        val bundledBinaryFileName = bundledBinaryFileName()
        val bundledBinaryResourcePath = "lsp/bin/$bundledBinaryFileName"
        val bundledBinaryStream = javaClass.classLoader.getResourceAsStream(bundledBinaryResourcePath) ?: return null
        val pluginSystemDirectory = Paths.get(PathManager.getSystemPath())
        val pluginCacheDirectory = pluginSystemDirectory.resolve(PLUGIN_CACHE_DIRECTORY)
        val extractedBinaryPath = pluginCacheDirectory.resolve(bundledBinaryFileName)

        Files.createDirectories(pluginCacheDirectory)

        bundledBinaryStream.use { inputStream ->
            Files.copy(inputStream, extractedBinaryPath, StandardCopyOption.REPLACE_EXISTING)
        }

        if (!SystemInfo.isWindows) {
            extractedBinaryPath.toFile().setExecutable(true)
        }

        if (!isExecutableServerBinary(extractedBinaryPath)) {
            return null
        }

        return extractedBinaryPath
    }

    private fun findProjectServerBinary(project: Project): Path? {
        val projectBasePath = project.basePath ?: return null
        val projectRootPath = Paths.get(projectBasePath)

        val candidateBinaryPaths = listOf(
            projectRootPath.resolve("target/release").resolve(bundledBinaryFileName()),
            projectRootPath.resolve("target/debug").resolve(bundledBinaryFileName()),
            projectRootPath.resolve("../target/release").resolve(bundledBinaryFileName()).normalize(),
            projectRootPath.resolve("../target/debug").resolve(bundledBinaryFileName()).normalize(),
            projectRootPath.resolve("../../target/release").resolve(bundledBinaryFileName()).normalize(),
            projectRootPath.resolve("../../target/debug").resolve(bundledBinaryFileName()).normalize(),
        )

        return candidateBinaryPaths.firstOrNull(::isExecutableServerBinary)
    }

    private fun bundledBinaryFileName(): String {
        if (SystemInfo.isWindows) {
            return "${EngineAiPluginConstants.SERVER_BINARY_NAME}.exe"
        }

        return EngineAiPluginConstants.SERVER_BINARY_NAME
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
