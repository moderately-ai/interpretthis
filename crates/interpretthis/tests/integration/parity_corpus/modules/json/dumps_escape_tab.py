# Horizontal tab must serialize as the `\t` escape rather than a literal byte.
import json
print(json.dumps("tab\tend"))
