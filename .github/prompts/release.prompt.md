---
description: 'Prepare the next release'
---
Your task is to prepare the next release for the project:
1. Analyze the recent changes in the codebase since the last tag, using git commit messages. Ensure to retrieve all commits, not just a subset, by using a command that lists all commit subjects or full log without patches if it causes truncation. For example, use `git log --oneline --reverse $(git describe --tags --abbrev=0)..HEAD` to get all commit subjects since the last tag.
2. Identify significant user-visible features, bug fixes, and improvements from the commit messages. Ignore internal refactors, translations updates unless they add new languages, and other non-user-facing changes.
3. Write clear and concise release notes summarizing only the user-visible changes, appending them in the appropriate release notes file (e.g., metainfo.xml.in or CHANGELOG.md) with the current date. Add the new release entry at the top of the releases section.
4. Determine the next version number following semantic versioning principles (major.minor.patch), based on the types of changes (new features: minor, breaking changes: major, fixes: patch).
5. Update the version number in the build configuration file (e.g., meson.build or package.json).
6. Do a final git commit with the message "vX.Y.Z" for the updated files. Finally, tag the commit with "vX.Y.Z". Use commands like `git add -A && git commit -m "vX.Y.Z"` and `git tag vX.Y.Z`.
