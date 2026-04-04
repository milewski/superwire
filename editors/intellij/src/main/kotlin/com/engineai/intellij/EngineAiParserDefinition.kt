package com.engineai.intellij

import com.intellij.lang.ASTNode
import com.intellij.lang.ParserDefinition
import com.intellij.lang.PsiParser
import com.intellij.lexer.Lexer
import com.intellij.openapi.project.Project
import com.intellij.psi.FileViewProvider
import com.intellij.psi.PsiElement
import com.intellij.psi.PsiFile
import com.intellij.psi.tree.IFileElementType
import com.intellij.psi.tree.TokenSet
import com.intellij.extapi.psi.ASTWrapperPsiElement

class EngineAiParserDefinition : ParserDefinition {
    override fun createLexer(project: Project?): Lexer {
        return EngineAiLexer()
    }

    override fun createParser(project: Project?): PsiParser {
        return PsiParser { rootElementType, psiBuilder ->
            val rootMarker = psiBuilder.mark()

            while (!psiBuilder.eof()) {
                psiBuilder.advanceLexer()
            }

            rootMarker.done(rootElementType)
            psiBuilder.treeBuilt
        }
    }

    override fun getFileNodeType(): IFileElementType {
        return EngineAiElementTypes.FILE
    }

    override fun getCommentTokens(): TokenSet {
        return TokenSet.EMPTY
    }

    override fun getStringLiteralElements(): TokenSet {
        return TokenSet.EMPTY
    }

    override fun createElement(node: ASTNode): PsiElement {
        return ASTWrapperPsiElement(node)
    }

    override fun createFile(viewProvider: FileViewProvider): PsiFile {
        return EngineAiPsiFile(viewProvider)
    }

    override fun spaceExistenceTypeBetweenTokens(left: ASTNode, right: ASTNode): ParserDefinition.SpaceRequirements {
        return ParserDefinition.SpaceRequirements.MAY
    }
}

class EngineAiPsiFile(viewProvider: FileViewProvider) : com.intellij.extapi.psi.PsiFileBase(viewProvider, EngineAiLanguage) {
    override fun getFileType() = EngineAiFileType

    override fun toString(): String {
        return "Engine AI File"
    }
}
