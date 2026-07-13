# typing.cast is a runtime no-op that returns its second argument.
# Pins the typing::call cast path.
from typing import cast
print(cast(int, 42))
print(cast(str, "hello"))
print(cast(list, [1, 2, 3]))
