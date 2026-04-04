package com.engineai.intellij

import com.intellij.lang.Language
import org.intellij.plugins.markdown.injection.aliases.AdditionalFenceLanguageSuggester

class EngineAiAdditionalFenceLanguageSuggester : AdditionalFenceLanguageSuggester {
    override fun suggestLanguage(name: String): Language? {
        val normalizedLanguageName = name
            .trim()
            .substringBefore(' ')
            .substringBefore('{')
            .lowercase()

        return if (normalizedLanguageName in supportedFenceTags) {
            EngineAiLanguage
        } else {
            null
        }
    }

    private companion object {
        val supportedFenceTags = setOf(
            "ai",
            "engine-ai",
            "engineai",
            "ai-dsl",
        )
    }
}
