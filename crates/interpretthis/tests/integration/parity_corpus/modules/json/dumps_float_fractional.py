# json.dumps on a fractional float renders with the bare decimal repr.
import json
print(json.dumps(2.5))
