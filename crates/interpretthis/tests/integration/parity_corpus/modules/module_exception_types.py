# Module-specific exception types are nameable in `except` clauses and behave
# like their CPython classes: the bare `__name__`, the subclass relationship to
# ValueError (statistics/json) or Exception (re), and the bare-name repr.
# Regression: `except statistics.StatisticsError:` etc. raised AttributeError
# because no module exposed its error type as an attribute.
import statistics
import json
import re

try:
    statistics.median([])
except statistics.StatisticsError as e:
    print("stat:", type(e).__name__, isinstance(e, ValueError), repr(e))

# A ValueError handler also catches it (StatisticsError subclasses ValueError).
try:
    statistics.mean([])
except ValueError as e:
    print("stat-as-value:", type(e).__name__)

try:
    json.loads("{bad}")
except json.JSONDecodeError as e:
    print("json:", type(e).__name__, isinstance(e, ValueError))

try:
    json.loads("[1,")
except ValueError as e:
    print("json-as-value:", type(e).__name__)

try:
    re.compile("(")
except re.error as e:
    print("re:", type(e).__name__, isinstance(e, ValueError), isinstance(e, Exception))

# Explicit construction and raise via the module attribute.
try:
    raise statistics.StatisticsError("boom")
except statistics.StatisticsError as e:
    print("raised:", type(e).__name__, str(e))
