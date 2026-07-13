# Pin: bare `import <module>` binds the module name and exposes its attributes.
# Expected stdout: `2.0`.
import math
print(math.sqrt(4))
