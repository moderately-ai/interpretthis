# None as a dict key coerces to the literal string `"null"`.
import json
print(json.dumps({None: "nothing"}))
