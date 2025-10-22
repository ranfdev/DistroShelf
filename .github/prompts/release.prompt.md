---
mode: agent
---
Your task is to prepare the next release:
1. Analyze the recent changes in the codebase, using git commit messages and diffs.
2. Identify significant features, bug fixes, and improvements.
3. Write clear and concise release notes summarizing these changes, appending them in data/com.ranfdev.DistroShelf.metainfo.xml.in. Ensure you add the new release entry at the top of the releases section with the current date.
4. Look at the version history in data/com.ranfdev.DistroShelf.metainfo.xml.in to determine the next version number, following semantic versioning principles.
5. Update the version number in the root meson.build
6. Do a final git commit with the message "vX.Y.Z". Finally, tag the commit with "vX.Y.Z".