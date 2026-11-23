```python
def is_prime(n: int) -> bool:
    """Check if a number is prime."""
    if n <= 1:
        return False
    if n <= 3:
        return True
    if n % 2 == 0 or n % 3 == 0:
        return False
    i = 5
    while i * i <= n:
        if n % i == 0 or n % (i + 2) == 0:
            return False
        i += 6
    return True
```

This function first handles edge cases by returning `False` for numbers less than or equal to 1 and `True` for 2 and 3, then eliminates multiples of 2 and 3 early for efficiency. It then iterates from 5 up to the square root of `n`, checking divisibility by `i` and `i + 2` (which covers all primes greater than 3) in steps of 6. If no divisors are found, the number is confirmed to be prime and the function returns `True`.
A refrigerator keeps food cold by moving heat out of the inside and dumping it into the room, rather than creating coldness. It uses a special liquid called refrigerant that circulates through coils inside the fridge. As this liquid flows, it absorbs heat from the air and food, causing the liquid to evaporate into a gas. This process cools the interior space.

Next, a compressor squeezes this warm gas, making it even hotter and pressurized. This hot gas then travels through coils on the back or bottom of the fridge. Here, a fan helps blow room-temperature air over the coils, allowing the heat to escape into your kitchen. As the gas loses its heat, it turns back into a cool liquid. This cycle repeats constantly, acting like a heat pump that continuously transfers warmth from the inside of the fridge to the outside environment. By constantly removing heat, the refrigerator maintains a low temperature that slows down bacterial growth and keeps your food fresh for longer.
```json
[
  {
    "country": "France",
    "capital": "Paris"
  },
  {
    "country": "Germany",
    "capital": "Berlin"
  },
  {
    "country": "Italy",
    "capital": "Rome"
  },
  {
    "country": "Spain",
    "capital": "Madrid"
  },
  {
    "country": "United Kingdom",
    "capital": "London"
  },
  {
    "country": "Poland",
    "capital": "Warsaw"
  },
  {
    "country": "Netherlands",
    "capital": "Amsterdam"
  },
  {
    "country": "Belgium",
    "capital": "Brussels"
  },
  {
    "country": "Greece",
    "capital": "Athens"
  },
  {
    "country": "Portugal",
    "capital": "Lisbon"
  }
]
```
Here is a five-point summary of the text:

*   **Water Sourcing and Intake:** Cities select water sources such as reservoirs or groundwater wells based on volume, cleanliness, and distance, with raw water first passing through screens to remove large debris like leaves and fish.
*   **Coagulation and Filtration:** Chemicals are added to clump tiny particles together (floc), which then settle out in tanks; the remaining water is filtered through sand, gravel, and carbon to remove fine particles, tastes, and odors.
*   **Disinfection and Chemical Adjustment:** Plants use chlorine,
