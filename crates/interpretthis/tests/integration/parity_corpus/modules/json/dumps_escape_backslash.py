# A literal backslash must serialize as the doubled `\\` escape.
import json
print(json.dumps("back\\slash"))
