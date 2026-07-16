# str/repr of datetime & time show microseconds (6 digits) when non-zero, and
# strftime %f is 6-digit microseconds (not chrono's 9-digit nanoseconds).
from datetime import datetime, time
dt = datetime(2024, 1, 15, 14, 30, 45, 123456)
print(dt)
print(str(dt), dt.isoformat())
print(dt.strftime("%Y-%m-%d %H:%M:%S.%f"))
print(dt.strftime("%H:%M:%S"), dt.strftime("%f"))
print(dt.time(), dt.replace(year=2025))
print(datetime(2024, 1, 15, 14, 30, 45))          # no micros -> no fractional
print(datetime(2024, 6, 1, 0, 0, 0, 1))
t = time(14, 30, 45, 500000)
print(t, str(t), t.isoformat())
print(time(14, 30, 45), time(0, 0, 0, 999999))
print(datetime(2024, 1, 1, 12, 30, 45, 678900).strftime("%f"))
print(dt.strftime("%A %B %d %I:%M %p"))
print(datetime(2024, 3, 15).strftime("%j"))
