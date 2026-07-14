# re.split interleaves captured groups from the split pattern into the result.
# Regression: captured groups were dropped, so `re.split(r'(\s)', ...)` lost the
# separators.
import re

print(re.split(r'(\s)', 'a b c'))
print(re.split(r'(,)(\s)', 'a, b, c'))
print(re.split(r',', 'a,b,c'))            # no groups -> plain pieces
print(re.split(r'(-)', 'a-b-c', maxsplit=1))
print(re.split(r'(x)|(y)', 'axbyc'))      # a group that doesn't participate -> None
print(re.split(r'\W+', 'a, b; c'))        # non-capturing separator
