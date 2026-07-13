# floor of NaN raises ValueError ("cannot convert float NaN to integer").
import math

print(math.floor(float('nan')))
