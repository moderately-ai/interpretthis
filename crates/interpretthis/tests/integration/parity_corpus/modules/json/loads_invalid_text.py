# Non-JSON input raises in CPython; our interpreter must propagate the failure too.
import json
json.loads('not json')
