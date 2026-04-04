package com.engineai.intellij

import com.intellij.lang.InjectableLanguage
import com.intellij.lang.Language

object EngineAiLanguage : Language(EngineAiPluginConstants.LANGUAGE_ID), InjectableLanguage {
    override fun getDisplayName(): String {
        return EngineAiPluginConstants.LANGUAGE_NAME
    }
}
