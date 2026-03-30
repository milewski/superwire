package com.engineai.intellij

import com.intellij.lang.Language

object EngineAiLanguage : Language(EngineAiPluginConstants.LANGUAGE_ID) {
    override fun getDisplayName(): String {
        return EngineAiPluginConstants.LANGUAGE_NAME
    }
}
