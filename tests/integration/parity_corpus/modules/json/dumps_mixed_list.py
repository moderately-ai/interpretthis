# A heterogeneous list flushes every scalar encoder inside an array context.
import json
print(json.dumps([1, "two", 3.0, True, None]))
