# re.sub count / re.split maxsplit distinguish 0 (unlimited) from negative
# (zero effect). Regression: both folded to 0, so a negative count replaced all.
import re

print(re.sub("a", "b", "aaa", count=0))      # bbb (all)
print(re.sub("a", "b", "aaa", count=-1))     # aaa (none)
print(re.sub("a", "b", "aaa", count=-5))     # aaa
print(re.sub("a", "b", "aaa", count=2))      # bba

print(re.split("a", "aaa", maxsplit=0))      # ['', '', '', ''] (all)
print(re.split("a", "aaa", maxsplit=-1))     # ['aaa'] (none)
print(re.split("a", "aaa", maxsplit=1))      # ['', 'aa']
print(re.split(r"(,)", "a,b,c", maxsplit=-1))  # ['a,b,c'] with groups suppressed
