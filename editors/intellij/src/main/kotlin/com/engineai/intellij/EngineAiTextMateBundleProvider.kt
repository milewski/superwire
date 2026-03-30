package com.engineai.intellij

import com.intellij.openapi.application.PluginPathManager
import org.jetbrains.plugins.textmate.api.TextMateBundleProvider
import org.jetbrains.plugins.textmate.api.TextMateBundleProvider.PluginBundle

class EngineAiTextMateBundleProvider : TextMateBundleProvider {
    override fun getBundles(): List<PluginBundle> {
        val pluginResourcePath = PluginPathManager.getPluginResource(javaClass, EngineAiPluginConstants.TEXTMATE_BUNDLE_DIRECTORY)

        if (pluginResourcePath == null) {
            return emptyList()
        }

        return listOf(PluginBundle(EngineAiPluginConstants.LANGUAGE_NAME, pluginResourcePath.toPath()))
    }
}
