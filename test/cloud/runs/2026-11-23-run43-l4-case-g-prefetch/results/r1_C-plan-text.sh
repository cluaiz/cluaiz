set -u
echo '### prompt code' >&2
printf %s 'Write a Python function that checks whether a number is prime, then explain it in three sentences.' > "$PDIR/code.txt"
"$CLUAIZ" -vv run "$ID" --local -f "$PDIR/code.txt" -n 256 --temp 0 --think off --mtp off "$@"
echo '### end' >&2
echo '### prompt chat' >&2
printf %s 'Explain how a refrigerator keeps food cold, in plain words, in about 150 words.' > "$PDIR/chat.txt"
"$CLUAIZ" -vv run "$ID" --local -f "$PDIR/chat.txt" -n 256 --temp 0 --think off --mtp off "$@"
echo '### end' >&2
echo '### prompt list' >&2
printf %s 'List ten countries in Europe with their capital cities as a JSON array of objects with the keys country and capital.' > "$PDIR/list.txt"
"$CLUAIZ" -vv run "$ID" --local -f "$PDIR/list.txt" -n 256 --temp 0 --think off --mtp off "$@"
echo '### end' >&2
echo '### prompt long' >&2
printf %s 'Read the text below, then summarize it in five bullet points.

A city water system starts far from the taps that use it. Rain and snow fall over hills and forests, soak into the soil, and run into streams that feed a river or a reservoir. Engineers choose a source by how much water it gives in a dry year, how clean it is before treatment, and how far it is from the people who need it. A reservoir behind a dam can store a winter of rain for a dry summer, while wells that reach groundwater give a steadier flow that changes slowly with the seasons.

Raw water first passes through screens that stop branches, leaves and fish. It then enters a mixing basin where a coagulant such as aluminium sulphate is added. The chemical makes tiny particles of clay and organic matter stick together into larger clumps called floc. Slow paddles keep the water moving gently so the floc grows without breaking apart. After about half an hour the water flows into a settling tank, where the heavy floc sinks to the bottom and is scraped away as sludge.

The clearer water then runs through filters made of layers of sand, gravel and sometimes activated carbon. Sand traps the remaining fine particles, and carbon removes some tastes, smells and dissolved chemicals. Filters slowly clog, so operators wash them backwards with clean water every day or two, lifting the trapped dirt out of the sand. The amount of dirt in the water after the filters, measured as turbidity, tells the operators whether the plant is working as it should.

Disinfection comes next. Most plants add chlorine, which kills bacteria and viruses and stays in the water as it travels through the pipes, protecting it on the way to homes. Some plants use ultraviolet light or ozone first, because they are better at killing certain parasites that resist chlorine, and then add a small dose of chlorine for the pipes. The plant keeps the chlorine level high enough to protect the water but low enough that people do not notice a strong taste.

Before the water leaves the plant, its acidity is adjusted. Water that is too acidic slowly dissolves metal from old pipes, including lead in some older houses, so operators raise the pH slightly and may add a phosphate that forms a thin protective layer inside the pipes. Many cities also add a small, controlled amount of fluoride to reduce tooth decay. Every step is recorded, and samples are taken many times a day and tested in a laboratory.

From the plant, large pumps push water into a network of mains under the streets. Storage tanks on hills or towers keep the pressure steady and hold a reserve for fires and for the busy hours in the morning and evening, when many people shower and cook at the same time. Pressure that is too low lets dirty water leak into the pipes, while pressure that is too high bursts old mains, so engineers divide a city into zones and control each with valves and pumps.

Leaks are one of the largest costs of a water system. In some cities a quarter of the treated water never reaches a customer because it escapes through cracked pipes and loose joints. Crews listen for leaks with sensitive microphones at night, when the streets are quiet, and use meters on each zone to find areas where more water goes in than comes out. Replacing old pipes is expensive and slow, so most cities replace a small share of their network each year.

After use, water from sinks, showers and toilets flows through a separate network of sewers to a treatment plant. There, the process is in some ways the reverse of drinking water treatment: solids are removed, bacteria are encouraged to eat the dissolved organic matter, and the cleaned water is released into a river or the sea, sometimes after a final disinfection. In dry regions, some cities treat this water further and send it back to recharge groundwater or to irrigate parks, closing the loop between the tap and the drain.' > "$PDIR/long.txt"
"$CLUAIZ" -vv run "$ID" --local -f "$PDIR/long.txt" -n 128 --temp 0 --think off --mtp off "$@"
echo '### end' >&2
