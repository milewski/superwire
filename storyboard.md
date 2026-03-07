provider ollama {
    api_endpoint <- "http://100.76.5.36:11434"
    models <- ["qwen3.5:27b"]
}

agent defaults {
    model <- "ollama/qwen3.5:27b"
}

agent storywriter {
    tools <- [tool.ask_user]
    output <- schema Story {
        title: String
        story: String
        setting: String
        moral: String
    }

    prompt file "./prompts/storywriter.md" {
        system """
            You are a creative assistant for children's stories.
            Ask the user simple questions to gather everything needed for the story.
            Keep asking until you have enough detail about the characters, setting, tone, and main idea.
            Then write a long detailed children's story.
            use the ask_user tool to ask for any missing information you need to complete the story.
        """
    }
}

agent character_analyzer {
    output <- schema {
        characters: [{
            name: String
            type: Enum("animal", "human", "creature", "object")
            appearance_prompt: String
            personality: String
            role: String
        }]
    }

    prompt file "./prompts/character_analyzer.md" {
        story_title <- agent.storywriter.title
        story_content <- agent.storywriter.story

        system """
            Read the story and extract all important characters.
            For each character, describe appearance, personality, and role.
            Create a clean image-generation prompt describing the character's visual appearance.
        """
    }
}

agent character_image_generator {
    for_each <- agent.character_analyzer.characters as character
    tools <- [tool.comfyui]
    output <- schema {
        character_name: String
        image_url: String
        prompt_used: String
    }

    prompt file "./prompts/character_image_generator.md" {
        character_name <- character.name
        appearance_prompt <- character.appearance_prompt

        system """
            Generate a character portrait image using the provided appearance prompt.
            Return the generated image url and the final prompt used.
        """
    }
}

<- agent book_assembler {
    tools <- []
    output <- schema {
        title: String
        pages: [{
            page_number: Number
            page_text: String
            illustration_url: String
        }]
    }

    prompt file "./prompts/book_assembler.md" {
        story_title <- agent.storywriter.title
        story_content <- agent.storywriter.story
        story_moral <- agent.storywriter.moral
        character_profiles <- agent.character_analyzer.characters
        character_images <- agent.character_image_generator

        system """
            You are responsible for creating a complete children's book with illustrations.

            Your task:
            1. Break the story into 6-10 pages, with each page containing 1-2 paragraphs
            2. For each page, identify the main character that should be illustrated
            3. Generate an illustration for each page using the comfyui tool
            4. Assemble the final book with all pages including text and illustration URLs

            Use the character reference images to maintain consistency across illustrations.
            Each page illustration should match the scene described in the text.

            Return the complete book as structured JSON with title and all pages.
        """
    }
}
