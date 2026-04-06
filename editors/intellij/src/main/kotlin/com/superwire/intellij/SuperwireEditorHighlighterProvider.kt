package com.superwire.intellij

import com.intellij.openapi.editor.colors.EditorColorsScheme
import com.intellij.openapi.editor.ex.util.DataStorage
import com.intellij.openapi.editor.ex.util.LexerEditorHighlighter
import com.intellij.openapi.editor.highlighter.EditorHighlighter
import com.intellij.openapi.fileTypes.EditorHighlighterProvider
import com.intellij.openapi.fileTypes.FileType
import com.intellij.openapi.fileTypes.SyntaxHighlighter
import com.intellij.openapi.fileTypes.SyntaxHighlighterFactory
import com.intellij.openapi.project.Project
import com.intellij.openapi.vfs.VirtualFile
import org.jetbrains.plugins.textmate.language.syntax.highlighting.TextMateHighlighter
import org.jetbrains.plugins.textmate.language.syntax.lexer.TextMateLexerDataStorage

class SuperwireEditorHighlighterProvider : EditorHighlighterProvider {
    override fun getEditorHighlighter(
        project: Project?,
        fileType: FileType,
        virtualFile: VirtualFile?,
        colors: EditorColorsScheme,
    ): EditorHighlighter {
        val syntaxHighlighter = SyntaxHighlighterFactory.getSyntaxHighlighter(fileType, project, virtualFile)

        return SuperwireLexerEditorHighlighter(syntaxHighlighter, colors)
    }

    private class SuperwireLexerEditorHighlighter(
        syntaxHighlighter: SyntaxHighlighter?,
        colors: EditorColorsScheme,
    ) : LexerEditorHighlighter(syntaxHighlighter ?: TextMateHighlighter(null), colors) {
        override fun createStorage(): DataStorage {
            return TextMateLexerDataStorage()
        }
    }
}
