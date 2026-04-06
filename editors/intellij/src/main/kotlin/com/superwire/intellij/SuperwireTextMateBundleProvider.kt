package com.superwire.intellij

import com.intellij.openapi.application.PathManager
import java.nio.file.Files
import java.nio.file.Path
import java.nio.file.Paths
import java.nio.file.StandardCopyOption
import org.jetbrains.plugins.textmate.api.TextMateBundleProvider
import org.jetbrains.plugins.textmate.api.TextMateBundleProvider.PluginBundle

class SuperwireTextMateBundleProvider : TextMateBundleProvider {
    private companion object {
        const val TEXTMATE_CACHE_DIRECTORY = "superwire-textmate"

        val bundledTextMateFiles = listOf(
            "package.json",
            "language-configuration.json",
            "syntaxes/wire.tmLanguage.json",
        )
    }

    override fun getBundles(): List<PluginBundle> {
        val extractedBundleDirectory = extractTextMateBundleDirectory()

        if (extractedBundleDirectory == null) {
            return emptyList()
        }

        return listOf(PluginBundle(SuperwirePluginConstants.LANGUAGE_NAME, extractedBundleDirectory))
    }

    private fun extractTextMateBundleDirectory(): Path? {
        val pluginSystemDirectory = Paths.get(PathManager.getSystemPath())
        val textMateCacheDirectory = pluginSystemDirectory.resolve(TEXTMATE_CACHE_DIRECTORY)
        val extractedBundleDirectory = textMateCacheDirectory.resolve(SuperwirePluginConstants.TEXTMATE_BUNDLE_DIRECTORY)

        Files.createDirectories(extractedBundleDirectory)

        for (bundledTextMateFilePath in bundledTextMateFiles) {
            val bundledResourcePath = "${SuperwirePluginConstants.TEXTMATE_BUNDLE_DIRECTORY}$bundledTextMateFilePath"
            val bundledResourceStream = javaClass.classLoader.getResourceAsStream(bundledResourcePath) ?: return null
            val extractedResourcePath = extractedBundleDirectory.resolve(bundledTextMateFilePath)

            Files.createDirectories(extractedResourcePath.parent)

            bundledResourceStream.use { inputStream ->
                Files.copy(inputStream, extractedResourcePath, StandardCopyOption.REPLACE_EXISTING)
            }
        }

        return extractedBundleDirectory
    }
}
