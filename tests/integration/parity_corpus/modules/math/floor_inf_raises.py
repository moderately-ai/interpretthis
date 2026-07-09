# floor of +inf raises OverflowError ("cannot convert float infinity to integer").
import math

print(math.floor(float('inf')))
