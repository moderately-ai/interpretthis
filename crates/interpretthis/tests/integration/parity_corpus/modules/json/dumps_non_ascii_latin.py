# Non-ASCII Latin-1 codepoints are escaped as `\uXXXX` per ensure_ascii=True (default).
import json
print(json.dumps("café"))
