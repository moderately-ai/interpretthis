# json.loads decodes JSON object into a Python dict; bool maps to True/False.
import json
print(json.loads('{"a": 1, "b": true}'))
