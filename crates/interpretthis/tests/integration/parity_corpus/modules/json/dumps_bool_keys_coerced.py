# Bool keys coerce to `"true"`/`"false"` strings, not the value forms.
import json
print(json.dumps({True: "yes", False: "no"}))
