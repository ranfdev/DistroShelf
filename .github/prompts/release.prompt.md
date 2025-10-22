---
mode: agent
---
Your task is to prepare the next release:
1. Analyze the recent changes in the codebase, using git commit messages and diffs.
Use the following git commands to gather the necessary information:
```sh
git log --reverse $(git describe --tags --abbrev=0)..HEAD --pretty=format:"commit %H%nAuthor: %an <%ae>%nDate: %ad%nSubject: %s%n%n%b" --date=short -p --no-color
```
2. Identify significant features, bug fixes, and improvements.
3. Write clear and concise release notes summarizing these changes, appending them in data/com.ranfdev.DistroShelf.metainfo.xml.in. Ensure you add the new release entry at the top of the releases section with the current date.
4. Look at the version history in data/com.ranfdev.DistroShelf.metainfo.xml.in to determine the next version number, following semantic versioning principles.
5. Update the version number in the root meson.build
6. Do a final git commit with the message "vX.Y.Z". Finally, tag the commit with "vX.Y.Z".