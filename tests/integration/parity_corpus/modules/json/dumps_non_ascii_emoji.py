# Astral-plane codepoints emit a UTF-16 surrogate pair under ensure_ascii=True.
import json
print(json.dumps("emoji: 😀"))
