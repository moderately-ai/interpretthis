# `{bad}` is not a valid JSON object — CPython raises JSONDecodeError; we must error too.
import json
json.loads('{bad}')
