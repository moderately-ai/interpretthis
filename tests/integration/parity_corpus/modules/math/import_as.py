# Pin: `import <module> as <alias>` binds the module under the alias.
# Expected stdout: `3.141592653589793`.
import math as m
print(m.pi)
