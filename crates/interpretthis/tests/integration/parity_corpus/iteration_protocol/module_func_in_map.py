# Pins: `map(json.dumps, items)` — ModuleFunction passed as the map
# function. Routes through call_value_as_function, which has no
# ModuleFunction arm today and would error "'ModuleFunction' object
# is not callable".
import json
print(list(map(json.dumps, [{"a": 1}, {"b": 2}])))
