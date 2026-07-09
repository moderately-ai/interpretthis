# Nested decode exercises dict-in-dict-in-list with `null` mapped to None.
import json
print(json.loads('{"nested": {"x": [1, 2.5, null]}}'))
