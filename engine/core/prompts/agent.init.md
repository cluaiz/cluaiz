---
name: agent.init
description: What /init asks the terminal agent to do, as a user message. {{path}} is where the instructions go (the project's .agents/AGENTS.md).
variables: [path]
---
Set up this project for AI agents: write {{path}}, the instructions an agent needs to work in this folder. First look at the project (paths relative to the working folder): list_dir "." and its main subfolders, and read the README and the files that say what the project is. For code these are the build or package files (for example Cargo.toml, package.json, pyproject.toml, Makefile); for documents, notes or spreadsheets, a few of the documents themselves. Then write {{path}} with write_file, under 60 lines.

For a code project use these sections: Project (what it is and its main folders), Build and test (the exact commands), Code style, and Rules (what to avoid).
For a folder of documents use these sections: Project (what the documents are about and how they are organized), Conventions (file names, formats, language and tone), Common tasks, and Rules (what not to change).

Write only what you found in the files; do not invent commands, folders or facts.
