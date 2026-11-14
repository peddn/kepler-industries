# Architektur und Regeln

Dieses Dokument hält fest, *warum* der Code so aufgebaut ist. Die meisten Regeln
stammen aus einer Analyse der technischen Probleme von Kerbal Space Program 1
und 2. Wer etwas ändert, das einer Regel widerspricht, ändert bitte zuerst
dieses Dokument (mit Begründung).

Das Dokument hat zwei Teile. Bis einschließlich „Stand und nächste Schritte“
steht, was gilt und gebaut ist. Unter „Ideen (nicht gebaut)“ steht, was
gesammelt, aber nicht beschlossen ist; nichts davon bindet den Code.

## Aufbau

| Crate | Aufgabe | Abhängigkeiten |
|---|---|---|
| `crates/sim` | Die gesamte Simulation: Himmelskörper, Bahnen, Atmosphäre, Schiffe, Zeit. Der Zustand (`World`) ist zugleich der Spielstand. | nur `glam` (f64-Mathe), `serde` und `serde_json` (Spielstand als Text) |
| `crates/game` | Darstellung und Eingabe mit Bevy. Liest die Simulation und treibt sie voran. | `sim`, `bevy`, `bevy_egui` (Oberfläche) |

`sim` kennt Bevy nicht. Dadurch kompiliert es in Sekunden, ist vollständig
ohne Fenster testbar (`cargo test -p sim`), und es gibt keinen Spielzustand,
der nur in Bevy-Komponenten steckt.

Einheiten: SI (m, s, kg), alles in `f64`. Zeit = Sekunden seit Spielbeginn.

## Regeln

### 1. Rechengenauigkeit
*KSP:* Unity rechnet in 32 Bit, KSP bräuchte 64 Bit. Folge: zitternde Schiffe,
der „Deep Space Kraken", aufwendige Gegenmaßnahmen (Floating Origin, Krakensbane).

**Regel:** Die Simulation rechnet nur in `f64`. Bevy bekommt jedes Bild nur
Positionen *relativ zum Kamerafokus* als `f32` (`camera::to_view`). Der Fokus
liegt in Bevy im Ursprung, die Kamera ist nie weiter entfernt als der
Zoomabstand. Es gibt keine `f32`-Weltkoordinaten. Deshalb geht die
Nahansicht bis 10 m an ein Schiff 700 km über dem Planetenmittelpunkt heran,
ohne dass etwas zittert.

### 2. Ein einziges Bezugssystem
*KSP:* Schaltet unterhalb von ~100 km auf ein mitrotierendes System um.
Folge: Geschwindigkeitsangaben weichen um die Rotationsgeschwindigkeit ab,
Positionen sind inkonsistent, Spieler vermuten Bahndrift in genau diesen Höhen.

**Regel:** Jede Bahn ist im nicht rotierenden System ihres Körpers
beschrieben (Ursprung im Mittelpunkt, +Z = Nordpol). Die Rotation steckt nur
in den Umrechnungen Oberfläche ↔ Raum in `body.rs`. Die Systeme von Planet
und Mond sind gegeneinander nur verschoben, nie gedreht; beim Wechsel der
Einflusssphäre wird die Lage um die Lage des Monds verschoben (`soi::convert`).

### 3. Keine Bahndrift
*KSP:* Bahnen werden bei Zeitraffer-, Fokus- und SOI-Wechseln neu berechnet und
gerundet; Satellitennetze laufen auseinander. KSP 2 hatte einen „Orbital
Decay"-Bug, der per Patch nur „stark reduziert" wurde.

**Regel:** Ein Keplerabschnitt speichert seinen Zustand zu Beginn (Epoche) und
verändert ihn nie. Jede Position wird direkt daraus berechnet
(`kepler::propagate`), bei Ellipsen nach Abzug ganzer Umläufe. Das
Newton-Verfahren dafür fällt auf eine eingegrenzte Suche zurück, wenn es aus
dem Ruder läuft (vorgekommen bei Hyperbeln knapp über der Parabel, *Test:*
`near_parabolic_hyperbola`). Neue Bahnen entstehen nur durch ausdrückliche
Ereignisse.
*Test:* `no_drift_after_thousand_orbits`, `stable_orbit_never_changes`.

### 4. Kein „aktives Schiff", keine Physik-Blase
*KSP:* Nur Schiffe im Umkreis von ~2,5 km werden voll simuliert. Andere fliegen
ohne Luftwiderstand oder werden in der Atmosphäre gelöscht. Für ein Spiel mit
automatischen Routen wäre das fatal.

**Regel:** Alle Schiffe werden gleich behandelt, egal ob sie im Bild sind.
Flugabschnitte mit Luft oder Schub werden einmal vorausberechnet und danach
nur abgespielt (`Segment::Integrated`).

### 5. Zeitraffer gilt immer
*KSP:* Kein Schub unter Zeitraffer, Ionentriebwerke brennen stundenlang in Echtzeit.

**Regel:** Weil berechnete Abschnitte nur abgespielt werden, funktioniert der
Zeitraffer in jeder Lage, auch in der Luft und beim Brennen.

### 6. Ergebnis unabhängig von der Schrittgröße
*KSP:* SOI-Wechsel im Hintergrund weichen bei hohem Zeitraffer von der Vorhersage ab.

**Regel:** Ereignisse (Eintritt in die Luft, Austritt, Bodenkontakt, Beginn und
Ende eines Brennmanövers) werden exakt bestimmt (Keplerbahn: analytisch,
berechneter Abschnitt: im Schritt interpoliert bzw. genau bis zum Zeitpunkt
gerechnet). Ein Abschnitt hängt nur von seinem Anfangszustand und dem
geplanten Manöver ab. Der Integrator hat eine feste Schrittweite, unabhängig
von Bildrate und Zeitraffer.
*Test:* `result_is_independent_of_step_size`, `node_result_is_independent_of_step_size`
(1 Schritt == 10 000 Schritte, bitgenau).

### 7. Keine Bauteil-Physik
*KSP:* Wackelnde Raketen, Lag ab einigen hundert Teilen, Phantomkräfte. In KSP 2
erzeugte ein Fix gegen das Wackeln einen wochenlang unbemerkten Treibstoff-Bug.

**Regel:** Ein Schiff ist ein Massepunkt mit Werten. Keine Physik-Engine.

### 8. Am Boden ist ein Zustand
*KSP:* Gelandete Schiffe hüpfen oder explodieren beim Laden.

**Regel:** „Am Boden" ist ein Abschnitt mit Körper, Breite und Länge
(`Segment::Surface`). Er dreht sich mit dem Körper und kann nicht hüpfen.

### 9. Viele Schiffe sind billig
*KSP:* Auch Schiffe „auf Schienen" und Trümmer kosten Rechenzeit.

**Regel:** Jedes Schiff kennt das Ende seines aktuellen Abschnitts. Pro Bild ist
das ein Zahlenvergleich. Gerechnet wird nur, wenn ein Abschnitt endet. Wracks
am Boden kosten nichts. Die einzige Wechselwirkung zwischen Schiffen ist das
Andocken: `World::advance_to` hält genau am frühesten Kontakt an, verbindet
die Schiffe und rückt dann weiter vor (eine Ereignis-Warteschlange in ihrer
kleinsten Form).

### 10. Kleine, spielbare Schritte
*KSP 2:* Studio 2024 geschlossen, versprochene Inhalte nie geliefert.

**Regel:** Jeder PR ist klein und spielbar. Die Simulation wird durch Tests
abgesichert, die ohne Bevy laufen.

### 11. Spielstände bitgenau
Der Test `save_and_load_round_trip` hat gezeigt: `serde_json` liest Zahlen
ohne die Option `float_roundtrip` nicht bitgenau zurück. Jedes künftige
Speicherformat muss Zahlen exakt erhalten. Der Spielstand trägt eine
Versionsnummer (`SAVE_VERSION`). Ältere Spielstände müssen vorerst nicht
laden (Entscheidung: Kompatibilität wird erst gepflegt, wenn es Spieler mit
wertvollen Ständen gibt); `sim::save` liest deshalb zuerst nur die
Versionsnummer und lehnt eine andere mit einer klaren Meldung ab. Im Spiel
(`savegame`): `F5` schreibt `spielstand.json` (erst in eine neue Datei, dann
umbenannt; der vorherige Stand bleibt als Sicherung), `F9` lädt. Bewusst kein
automatisches Speichern: Seit man sich festspielen kann, würde es einen guten
Stand überschreiben. Nach dem Laden wird alles zurückgesetzt, was auf Schiffe,
Nodes oder Zonen der alten Welt zeigt (Auswahl, Kamera-Ziel, Hinweise);
Markierungen richten sich nach der Zahl der Schiffe und Basen. Auch das Protokoll gehört dazu (`World::log`):
Die Einträge entstehen, wenn ein Abschnitt der Zeitleiste endet, mit dessen
genauem Zeitpunkt, und sind deshalb wie alles andere unabhängig von der
Schrittgröße (*Test:* `log_is_independent_of_step_size`).

### 12. Die Vorschau ist die Ausführung
Ein Manöver-Node ist der Plan selbst: `add_node`/`set_node` berechnen die
Zeitleiste neu, und diese Zeitleiste wird später genau so abgespielt. Es gibt kein eigenes
Vorschausystem, das von der Ausführung abweichen könnte.
*Test:* `impact_prediction_matches_wreck`, `clearing_node_restores_trajectory`.

### 13. Bedienelemente auf eigener Ebene
Node und Griffe zeichnet eine zweite Kamera mit eigenem Tiefenpuffer
(`OVERLAY_LAYER`). Sie liegen damit immer über der Welt und verschwinden nicht
im Planeten. Der Versuch mit `depth_bias = -1` zeichnete Linien nur
bruchstückhaft.

### 14. Zeit allein ändert nichts
Das Spiel lebt vom Zeitraffer (bis 1.000.000×, ein Flug nach Nura dauert
270 Tage, ein Startfenster kommt alle 900). Was allein durch Warten mehr oder
weniger würde, wäre damit entweder geschenkt oder eine Strafe fürs Fliegen.

**Regel:** Zeit allein erzeugt und vernichtet nichts. Alles in der Wirtschaft
hängt am Durchsatz: Stoffe und Daten entstehen und vergehen nur, wenn
gearbeitet, geliefert oder gehandelt wird. Stillstand kostet nichts und
bringt nichts. Erlaubt ist, was sich wiederholt, ohne sich aufzusummieren
(eine Batterie im Tag-Nacht-Wechsel). Es gibt keinen Druck durch die Uhr; das
Spiel darf gemütlich sein. Einzelheiten im Abschnitt „Wirtschaft“.

## Physikmodell

**Ein Kraftmodell überall:** Schwerkraft + Luftwiderstand + Schub.
Ein Körper ohne Luft ist der Spezialfall Bodendichte 0, Freiflug der
Spezialfall „kein Schub".

**Körper auf Schienen:** Planeten und Monde laufen wie in KSP auf einer
festen Keplerbahn um ihren Mutterkörper (`BodyOrbit`), beschrieben durch die sechs
Bahnelemente zum Zeitpunkt 0: große Halbachse, Exzentrizität, Neigung, Länge
des aufsteigenden Knotens, Argument der Periapsis, mittlere Anomalie. Seine
Lage wird für jeden Zeitpunkt direkt berechnet (`body::body_state`): Die
mittlere Anomalie wächst gleichmäßig (über den Rest der Umlaufzeit wie die
Rotation, damit auch nach Jahren genau), die exzentrische folgt aus der
Kepler-Gleichung (Newton). Mutterkörper kann jeder Körper sein, die Lage
wird rekursiv zusammengesetzt (Monde von Monden gingen also, sind aber nicht
getestet). Ein Körper wird nie integriert und spürt keine Schiffe. Die Sonne
Zunn (wie Kerbol) hat keine Bahn, ruht im Ursprung und reicht unendlich weit.
Nummern wie in KSP: erst die Sonne, dann der Heimatplanet (`World::home`, er
zählt die Tage, die Kamera beginnt bei ihm) mit seinen Monden, dann die
anderen Planeten. Zorb kreist wie Kerbin um Zunn (13.599.840 km, Jahr
9.203.545 s), Nura wie Duna (große Halbachse 20.726.155 km, Exzentrizität
0,051, Einflusssphäre 47.900 km, dünne Luft bis 50 km). Wie in KSP gibt es keine Achsneigung: Alle Körper
drehen sich um +Z, die Neigung einer Bahn gilt gegenüber dieser gemeinsamen
Äquatorebene. Die Einflusssphäre folgt nach Laplace aus großer Halbachse und
Massen: a · (m/M)^0,4 (`body::sphere_of_influence`, auch bei Ellipsen gleich
groß); darin zählt für ein Schiff nur die Schwerkraft des Monds (Patched
Conics, ab dem Wechsel der Einflusssphäre). Zoon entspricht dem Mun in KSP
(Kreis über dem Äquator, 200 km, g = 1,63 m/s², 12.000 km Abstand, Umlauf
38,6 h, Einflusssphäre 2.430 km) und ist gebunden: Rotation und Umlauf haben
dieselbe Dauer, Länge 0° zeigt immer zum Planeten (bei einer Ellipse
schwankte der Planet dabei am Himmel, Libration). Zill ist klein, aber extrem
dicht (100 km, g = 5,75 m/s², große Halbachse 47.000 km, Exzentrizität 0,25,
Neigung 15°, Einflusssphäre 9.050 km) und dreht sich frei. Er ist bewusst
das schwerere Ziel nach Zoon: Für Landung und Start zählt das Produkt aus
Schwerkraft und Radius (Fluchtgeschwindigkeit am Boden 1.070 m/s, Zoon
810 m/s); Körper sind durch Radius und Schwerkraft festgelegt, die Dichte
ist frei (und wie überall in diesem Maßstab weit über echten Werten).

**Wechsel der Einflusssphäre** (`soi.rs`): An der Grenze endet ein Abschnitt
(`SoiEntry`, `SoiExit`), der nächste läuft um den anderen Körper weiter, ein
Brennen ebenso (es wird geteilt, Richtung, Schub und Massenstrom bleiben).
Der Austritt ist auf der Keplerbahn analytisch
(`kepler::time_to_radius_outbound`). Den Eintritt in die Sphäre des Monds
muss man suchen, weil er sich bewegt: Der Abstand wird nah heran so fein
abgetastet, dass sich Schiff und Mond pro Schritt höchstens um ein Viertel
des Radius gegeneinander bewegen, weit weg in so großen Schritten, wie sie
bei höchster Geschwindigkeit mindestens bis zur Grenze brauchen (so wird kein
Eintritt übersprungen, und eine Bahn um die Sonne kostet nur wenige hundert
Punkte); ein Vorzeichenwechsel wird per Bisektion eingegrenzt
(auf 1 µs), ein tiefer Punkt zwischen zwei Abtastpunkten per Goldenem Schnitt
(knappe Streifschüsse). Gesucht wird nur, wenn der Abstand der Bahn zum
Planeten überhaupt in den Bereich fällt, den der Mond samt Sphäre zwischen
seiner Periapsis und Apoapsis überstreicht, und jeweils einen
Umlauf weit; ohne Treffer endet der Keplerabschnitt dort mit `Continue`, und
der nächste sucht weiter, wenn er an der Reihe ist. Bahnen in Planetennähe
bleiben ein einziger offener Abschnitt wie bisher. Schrittweise berechnete
Abschnitte prüfen beide Grenzen bei jedem Schritt, wie den Bodenkontakt.
Alles hängt nur vom Anfangszustand ab (Regel 6). Wer Zorbs Einflusssphäre
verlässt (84.159 km, wie Kerbin gegenüber Kerbol), fliegt um Zunn weiter;
der Eintritt in einen Planeten wird genauso gesucht wie der in einen Mond.
Ein Node um die Sonne geht wie jeder andere. Die Sonne verlässt man nie
(offene Bahnen um sie werden in Fenstern von 10 Mio. s weitergesucht).
Rendezvous, Bahnebene und Andocken zählen nur Zeiten, in denen beide Schiffe
um denselben Körper fliegen.

**Atmosphäre:** verschobene Exponentialfunktion

    ρ(h) = ρ₀ · (e^(−h/H) − e^(−h_max/H)) / (1 − e^(−h_max/H))   für h < h_max, sonst 0

Am Boden ρ₀, bei `h_max` exakt 0, dazwischen stetig. Oberhalb von `h_max` ist
die Keplerbahn deshalb die *exakte* Lösung desselben Modells, keine Näherung.
Eine reine Exponentialfunktion würde nie null; dann müsste jede Bahn ewig
schrittweise gerechnet werden (Drift, Rechenzeit).

**Luftwiderstand:** `a = −½ · ρ · (c_w·A / m) · |v_rel| · v_rel`, mit `v_rel`
relativ zur mitrotierenden Luft.

**Brennmanöver:** konstanter Schub, konstanter Massenstrom. Die Masse wird
analytisch aus der Zeit berechnet (nicht integriert), dadurch gilt die
Raketengleichung exakt. Die Dauer folgt aus dem Δv:
`m_end = m₀ · e^(−Δv/v_e)`, `Dauer = (m₀ − m_end) / ṁ`. Das Manöver liegt
zeitlich mittig um den Node, die Schubrichtung bleibt im Raum fest (wie in KSP
beim Zielen auf den Node). Reicht der Treibstoff nicht, endet es mit leerem Tank.

**Manöverkette:** Ein Schiff hat eine Kette von Nodes. Jeder liegt auf der
Bahn nach den vorherigen Brennvorgängen; die Zeitleiste rechnet sie der Reihe
nach aus (`Trajectory::rebuild`). Gespeichert wird pro Node, wohin er gesetzt
wurde (Zeit, Δv); Lage, Richtung und Brennzeiten folgen daraus (`Plan`).
Ändert sich ein früherer Node, behalten spätere Zeit und Δv und wandern mit
der Bahn.

**Brennen ganz in der Zukunft und nacheinander:** Ein Brennen beginnt nicht
vor jetzt und echt nach dem Ende des vorherigen. Sonst rückt der Node genau
einen Umlauf weiter (gleiche Stelle seiner Bahn, alle Zeiten + Umlaufzeit),
so oft wie nötig. Das geht nur auf einer geschlossenen Bahn, die bis zum Ende
des verschobenen Brennens weder Luft noch Boden berührt. Rückt ein Node nur
wegen eines früheren weiter, wird das jedes Mal neu berechnet, nicht
gespeichert; nimmt man das frühere Manöver zurück, kehrt er zurück.

**Ungültige Nodes:** Gibt es für einen Node auf seiner Bahn keinen Platz
(Absturz vorher `OnGround`, kein nächster Umlauf `TooLate`/`Overlaps`, hinter
der Vorausberechnung), ist er ungültig. Beim Setzen oder Ändern wird so ein
Node abgelehnt. Wird er es erst durch die Änderung eines früheren, bleibt er
erhalten, wird nicht ausgeführt und erscheint nur in der Liste (rot). Alle
späteren sind dann auch ungültig (`AfterInvalid`). Passt es wieder, ist er
wieder gültig.

**Nicht scharfe Kette:** Erreicht sie ihren ersten Brennbeginn, rückt sie als
Ganzes einen Umlauf weiter (nach derselben Regel) oder verfällt.

**Start vom Boden (Startprofil):** vier Werte: Kurs (90° = Osten), Wendebeginn
(Höhe), Wendewinkel, Ziel-Apoapsis. Eine Schwerkraftwende wie in KSP: senkrecht bis zum
Wendebeginn, dort um den Wendewinkel zur Seite, danach immer entlang der Bewegung
relativ zur Luft, nie steiler als der Wendewinkel, nie unter die Waagerechte.
Voreinstellung nach dem Körper (`LaunchProfile::for_body`): mit Luft 1,5 km
und 10° (steiler wenden fliegt zu lange durch dichte Luft und stürzt ab),
ohne Luft 100 m und 70° (auf Zoon mit Schub/Gewicht 12 stiege ein Schiff mit
10° fast senkrecht: 822 statt 666 m/s bis zur 30-km-Bahn; 85° spart noch
25 m/s, lässt schwache Schiffe aber kaum steigen). Ziel-Apoapsis 30 km über
der Luftgrenze bzw. dem Boden. 10° rechnet bitgenau wie der alte feste Winkel
(`kick_radians`), alte Spielstände bleiben gleich. Die Schwerkraft biegt die Bahn
von selbst um; früher wenden heißt flacher. Schub entlang der Bewegung hebt
die Apoapsis stetig, deshalb wird die Zielhöhe sicher erreicht, auch bei einem
Hop mit wenigen km. (Eine feste Neigung nach Höhe blieb bei einem Hop in der
Luft hängen und verbrannte den ganzen Tank.) Die Schubrichtung bleibt in einer
festen Ebene durch Startplatz und Kurs (Großkreis); die Bahnneigung ergibt
sich daraus und aus der Drehung des Planeten. Der Aufstieg
ist ein normales `Burn` mit Aufstiegssteuerung statt fester Richtung, deshalb
gelten Vorschau = Ausführung, Brennpunkte und Nodes danach ohne Sonderweg.
Brennschluss, wenn die Apoapsis die Zielhöhe erreicht: beim Planen mit
denselben Schritten wie die Zeitleiste gerechnet und im Schritt auf Bruchteile
eines Meters genau gesucht, dann als Ende des Brennens gespeichert. Danach
Freiflug; die obere Luft senkt die Apoapsis noch etwas, die Vorschau zeigt den
echten Wert. Mit leerem Tank vorher: Brennschluss dort, die Vorschau zeigt den
Absturz. Kein Start bei Schub ≤ Gewicht. Der Start steht vor allen Nodes und
gehört zur Kette; Nodes danach liegen auf der Bahn nach dem Aufstieg und
lassen sich schon vor dem Abheben setzen. Wird die Startzeit verschoben,
wandert die ganze Kette mit. Ein nicht scharfer Start wartet eine
Planetendrehung (dann liegt der Startplatz wieder gleich).

**Landung:** Die Vorschau rechnet bis zum Boden. Ein Fallschirm ist nur eine
größere Widerstandsfläche im selben Modell (`parachute_area`): offen unter
3 km im Sinkflug; die Regel hängt nur von Höhe und Richtung ab, die Simulation
muss sich nichts merken. Bis 5 m/s relativ zum Boden setzt jedes Schiff
auf, schneller wird es zum Wrack. Käme ein Freiflug schneller am Boden an,
setzt die Zeitleiste davor automatisch ein **Landebrennen** (`landing`): ein
normales Brennen (`Burn` mit Landesteuerung `Descent`, wie der Aufstieg mit
`Ascent`) mit vollem Schub gegen die Bewegung relativ zum Boden. Es beginnt so
spät wie möglich: Gesucht wird erst rückwärts in sich verdoppelnden
Schritten, dann per Bisektion, bis das Schiff höchstens so hoch zum Stehen
kommt, dass es aus dieser Höhe mit 2,5 m/s aufsetzt (auf Zorb etwa 30 cm, auf
Zoon 2 m). Jeder Versuch rechnet das Landebrennen genau wie die Zeitleiste
(dieselben Schritte ab demselben Zustand, auch mitten in einem Abschnitt
durch die Luft, `integrator::state_on_arc`). Sobald das Schiff steht, endet
das Brennen (`integrate_arc` erkennt das im Schritt; in diesem letzten
Teilschritt bleibt die Schubrichtung fest, sonst bliebe bei starkem Schub
ein Rest an Geschwindigkeit); der Rest ist ein kurzer
Fall. Treibstoff nach der Raketengleichung, Verluste durch Schwerkraft und
Luft inbegriffen, Vorschau = Ausführung bitgenau. Geht das nicht, wird nicht
gebremst (Absturz mit Grund, `Crash`): Schub nicht größer als das Gewicht
(Satelliten), zu wenig Treibstoff (mit dem, was mindestens fehlt), oder keine
Zeit mehr (der Freiflug beginnt zu knapp über dem Boden, oder ein geplantes
Brennen führt in den Boden). Das Landebrennen gehört nicht zur Kette, es kommt
immer (auch ohne scharfe Kette). Derselbe Code für jeden Körper, die
Unterschiede kommen nur aus seinen Daten: Auf Zorb endet die Keplerbahn an
der Luftgrenze und der Rest wird mit Luftwiderstand und Fallschirm gerechnet,
das Landebrennen ist dann eine Sekunde knapp über dem Boden; auf Zoon (ohne
Luft) bremst allein das Landebrennen (aus niedriger Bahn mit viel Schub etwa
550 m/s, bei Schub/Gewicht 1,5 etwa 700 m/s). Ob das Schiff in einem **Zielgebiet** (Kreis,
5 km Radius) aufsetzt, entscheidet nicht über heil oder kaputt; es zählt für
Basis, Andocken und Aufträge. Außerhalb steht das Schiff „außerhalb“ und
kommt mit einem Hop (Start mit niedriger Ziel-Apoapsis) ins Ziel. Nach dem
Aufsetzen ist die Kette erledigt.

**Körper als Ziel** (`transfer.rs`): reine Auswertung wie das Rendezvous.
Ein Schiff hat höchstens ein Ziel, ein Schiff oder einen Himmelskörper
(`Vessel::target`, `Vessel::target_body`). *Startfenster:* Für einen
Hohmann-Übergang muss das Ziel dem Ausgangspunkt um π·(1 − ((r₁+r₂)/(2r₂))^1,5)
vorauslaufen, gesehen vom gemeinsamen Mutterkörper (Zorb → Nura: 44°).
Ausgangspunkt ist der Körper, um den das Schiff kreist, oder das Schiff
selbst, wenn es schon um den Mutterkörper des Ziels kreist (Flug zu einem
Mond). Der Phasenwinkel ändert sich gleichmäßig, daraus folgt die Wartezeit.
Die Formel nimmt Kreisbahnen in einer Ebene an: ein Richtwert (bei Nura auf
wenige Tage). *Annäherung:* der kleinste Abstand der vorausberechneten Bahn
zum Zielkörper nach dem letzten geplanten Brennen, einen Umlauf weit auf der
Bahn, bei der die Zeitleiste ankommt (beim Flug zu einem Planeten der erste
Umlauf um die Sonne); führt die Bahn in seine Einflusssphäre, zählen Eintritt
und Periapsis. So zeigt die Karte beim Ziehen am Node, ob es besser wird,
lange bevor eine Begegnung zustande kommt.

**Rendezvous:** reine Auswertung der vorausberechneten Zeitleisten zweier
Schiffe (`rendezvous.rs`), nichts wird verändert. Engste Annäherung: ab jetzt
(am Boden ab dem Abheben, Zeit am Boden zählt nicht) bis vier Umläufe nach
dem letzten geplanten Brennen; grob abgetastet (2000 Stützstellen), jedes
Minimum mit dem Goldenen Schnitt genau gesucht. Nimmt der Abstand nur ab, ist
das Ende des Zeitraums die engste Stelle (wichtig fürs Aufholen auf fast
derselben Bahn). Bahnebene: Winkel zwischen den Drehimpulsen der eigenen Bahn
nach dem letzten Brennen und der Zielbahn; die Knoten (AN/DN) sind die
Vorzeichenwechsel des Abstands zur Zielebene, per Bisektion. Schwingt das
Schiff höchstens 1 m über und unter der Zielebene (`COPLANAR_HEIGHT`, bei
700 km Bahnradius etwa 0,0001°), gilt das als gleiche Ebene: keine Knoten,
sonst sprängen sie im Rundungsrauschen umher (etwa nach dem Abdocken, wo
beide Bahnen exakt in einer Ebene liegen). Weniger als 1 ms
pro Bild, deshalb ohne Zwischenspeicher. Das Ziel gehört zum Schiff und zum
Spielstand (`Vessel::target`).

**Andocken** (`docking.rs`): Der Spieler stellt mit normalen Nodes die
engste Annäherung ein; liegt sie unter 1 km, hängt „Andocken“ an die Kette:
1. **Angleichen:** ein automatischer Node an der engsten Annäherung (ohne
   das Andocken gesucht), Δv = Geschwindigkeit des Ziels minus eigene. Ein
   normales, zentriertes Brennen; es endet fast genau an der engsten Stelle.
2. **Anflug** (`Segment::Approach`): gerade Linie im mitbewegten System des
   Ziels (radial, entlang, normal) bis zu dessen Mitte; beschleunigen mit
   0,05 m/s² auf höchstens 2 m/s, Kontakt mit 0,1 m/s. Der Treibstoff ist die
   Summe des Schubs, den diese Linie gegen die Schwerkraft braucht, dazu der
   Rest der Relativgeschwindigkeit nach dem Angleichen. Der Abschnitt
   speichert eine Kopie der Zielbahn (Keplerzustand zu einer Epoche) und ist
   analytisch: keine Drift, unabhängig von der Schrittgröße.
3. **Kontakt:** Das Schiff wird Teil des Verbunds um das Ziel
   (`Vessel::docked_to`). Das Ziel bleibt Hauptschiff mit Name und Kette;
   seine wirksamen Werte (`World::params`) sind die Summe (gemeinsamer Tank,
   Summenschub, Ausströmgeschwindigkeit nach Massenstrom). Lage und Masse
   eines angedockten Schiffs sind die des Hauptschiffs; Befehle gehen an das
   Hauptschiff.

Bedingung: Das Ziel ist ab dem Angleichen auf einer stabilen Bahn und plant
nichts mehr (sein Keplerabschnitt hat kein Ende). Nur dann hängt seine Bahn
nicht von der Masse ab, und Vorschau = Ausführung bleibt exakt. Weil das
Andocken von zwei Schiffen abhängt, rechnet `World` es nach jedem Befehl an
irgendeiner Kette neu (`refresh_dockings`).

**Abdocken** (`World::undock`): ein Befehl für jetzt, im Flug und nicht
während eines Brennens. Das Teil nimmt einen einstellbaren Teil des
gemeinsamen Treibstoffs mit (höchstens sein Tank, mindestens so viel, dass der
Rest in die übrigen Tanks passt; `World::undock_fuel`). Die Masse bleibt
erhalten. Das Teil bekommt `SEPARATION_SPEED` (1 m/s) gegen die
Flugrichtung: Es sinkt minimal und läuft voraus, pro Umlauf etwa
3 · Umlaufzeit · Stoß (in 100 km Höhe 5,9 km). Beide Zeitleisten beginnen
neu; das Hauptschiff behält seine Kette, die mit den neuen Werten neu
berechnet wird (ungültig gewordene Nodes bleiben), das Teil fängt ohne Kette
an, mit dem Hauptschiff als Ziel.

**Zeitleiste eines Schiffs:** Folge von Abschnitten
- `Kepler`: Freiflug ohne Luft, analytisch (universelle Variablen: eine Formel
  für Ellipse, Parabel, Hyperbel).
- `Integrated`: vorausberechnet (RK4, 0,1 s, Stützpunkt jede Sekunde,
  Hermite-Interpolation dazwischen, max. 30 min pro Stück); mit oder ohne Schub.
- `Surface`: am Boden (gelandet oder Wrack); mit geplantem Start endet er beim
  Abheben.

Vorausberechnet werden 8 Abschnitte (`LOOKAHEAD`) nach dem Ende des letzten
geplanten Brennens (ohne Manöver: ab jetzt), oder bis die Zeitleiste offen
endet (stabile Bahn, am Boden). Das reicht, um nach dem letzten Manöver die
neue Bahn oder den Aufschlag zu sehen. Auf einer Bahn, die bei jedem Umlauf
durch die Luft führt, sind 8 Abschnitte vier Umläufe; weiter voraus lässt
sich kein Node setzen.
Übergänge: Kepler → (Eintritt) → Integrated → (Austritt) → Kepler;
Freiflug → (Brennbeginn) → Integrated mit Schub → (Brennende) → Freiflug;
Surface → (Abheben) → Integrated mit Aufstiegssteuerung → (Brennschluss) → Freiflug;
Freiflug → (Landebrennen) → Integrated mit Landesteuerung → (steht) → Freiflug;
→ (Bodenkontakt) → Surface (aufgesetzt oder Wrack, siehe Landung).

**Aufschlagkreuz:** Es markiert die Stelle *auf dem Boden* und dreht sich mit
dem Planeten. Die Bahnlinie ist im nicht rotierenden System gezeichnet. Bis
zum Aufschlag dreht sich der Planet noch, deshalb liegt das Kreuz vorher nicht
am Ende der Linie; beide treffen sich genau beim Aufschlag. Für den Vergleich
mit Zielgebieten (die sich ebenfalls mitdrehen) ist das die richtige Größe.

## Getroffene Spielentscheidungen

- 3D, kleine Planeten (Kerbin-Größe), **keine Raketenstufen**.
- **Namen:** Der Heimatplanet heißt Zorb, seine Monde Zoon und Zill, der
  zweite Planet Nura, die Sonne Zunn,
  die Bewohner Zorbels. Die Namen der Himmelskörper stehen nur im Szenario
  (`Body::name`), alle Anzeigen lesen sie von dort; alte Spielstände
  behalten ihre Namen.
- **Aufschlagkreuz und Bahnlinie:** Das Kreuz zeigt den genauen Punkt auf dem
  Boden (dreht mit dem Planeten), die Bahn wird ohne Planetendrehung
  gezeichnet. Dass das Ende der Linie vor dem Aufschlag nicht am Kreuz liegt,
  ist gewollt (siehe „Aufschlagkreuz“ unter Physikmodell).
- **Manöverketten:** mehrere Nodes pro Schiff (siehe „Manöverkette“ unter
  Physikmodell). Ein Klick auf einen beliebigen gezeichneten Teil der Zukunft
  fügt einen Node ein, auch auf die Bahn nach einem geplanten Manöver. Ein
  Klick auf einen Node (oder `↑`/`↓`) wählt ihn; nur der gewählte hat Griffe,
  `Entf` löscht ihn. Verschoben wird ein Node entlang seiner Bahn (der Bahn
  nach den früheren Manövern). Auf einer zerfallenden Bahn liegen spätere
  Umläufe weiter innen; zieht man dorthin, kommt der Node auf einen späteren
  Umlauf. Das zeigen das Etikett „+N Umläufe“ am gewählten Node und die Liste.
  Auf einer stabilen Bahn liegen die Umläufe übereinander, dort bleibt der
  Node im ersten (eine Schaltfläche „+1 Umlauf“ steht bei den UI-Ideen).
- **Scharf schalten pro Kette** (`Trajectory::armed`): Nur eine scharfe Kette
  wird ausgeführt. In der Simulation ist eine neue Kette nicht scharf. Eine
  nicht scharfe rückt bei Brennbeginn als Ganzes einen Umlauf weiter, so geht
  kein Plan verloren, nur weil der Zeitraffer läuft. Die Vorschau ist scharf
  wie nicht scharf dieselbe. Der Zeitraffer-Stopp greift nur bei scharfen
  Ketten (und beim Landebrennen, das immer kommt). Eine Kette passt auch zu späteren Routen. **Vorerst schaltet das
  Spiel jede neue Kette sofort scharf**; die Schaltfläche dafür kommt später
  an anderer Stelle in die Oberfläche (siehe UI-Ideen).
- **Ungültige Nodes bleiben erhalten** (siehe Physikmodell), damit beim
  Ausprobieren am frühen Node kein späterer Plan verloren geht.
- **Brennpunkte:** Gelbe Ringe auf der Bahn markieren Beginn und Ende jedes
  geplanten Brennens, für alle Schiffe und alle gültigen Nodes.
- **Griffe:** Ein gezogener Griff folgt der Maus entlang seiner Achse und
  springt beim Loslassen zurück (wie in KSP). Er kann das Δv nicht über den
  Tankinhalt am Brennbeginn hinaus erhöhen. Das begrenzt nur die Bedienung;
  die Simulation erlaubt mehr Δv als im Tank ist (das Brennen endet dann
  leer), damit später vor einem Node nachgetankt werden kann.
- **Während des Brennens** zeigt die Karte die momentane Bahn (die Keplerbahn
  aus dem aktuellen Zustand, wie in KSP). Reine Darstellung, die Simulation
  rechnet unverändert den ganzen Brennvorgang voraus. Nicht beim
  Landebrennen: Diese Bahn ginge nur in den Boden und verdeckte den gelben
  Weg nach unten.
- **Zeitraffer-Stopp pro Schiff** (`halt_warp_at_burn`, Taste `H`, anfangs aus):
  Beginnt ein markiertes Schiff zu brennen, hält der Zeitraffer genau am
  Brennbeginn an und schaltet auf 1×. Pro Schiff statt für alle, damit spätere
  automatische Routen den Zeitraffer nicht ständig bremsen. Die Markierung
  gehört zum Spielstand.
- **Startprofil** mit wenigen Parametern (Kurs, Wendebeginn, Wendewinkel, Ziel-Apoapsis);
  das Schiff kommt mit nicht mehr vollen Tanks oben an. Zirkularisieren per
  Manöver-Node, auch schon vor dem Abheben geplant. Feste Startzeit mit
  Countdown (Vorgabe 2 min); verschieben verschiebt die ganze Kette.
- **Kamera:** `F` richtet sie auf das gewählte Objekt (gewählter Node, sonst
  Schiff, sonst Zielgebiet oder Mond, ohne Auswahl den Planeten), ein zweites `F` auf
  dasselbe zurück zum Planeten. Der Fokus ist unabhängig von der Auswahl: Die
  Kamera bleibt auf ihrem Objekt und folgt ihm, auch wenn man danach etwas
  anderes wählt (z. B. Zielgebiet im Blick, Brems-Node des Schiffs
  einstellen). Zielgebiete sind über eine Raute in der Mitte wählbar, der
  Mond per Klick auf die Kugel (erst nach der Bahn des gewählten Schiffs, die
  oft davor liegt). In der Nahansicht zeigt "oben" vom Körper weg, bei dem
  das Objekt gerade ist (auch vom Mond). Auf Planet und Mond gibt es keine Nahansicht; der
  kleinste Abstand liegt knapp über der Oberfläche. Gibt es
  das Objekt nicht mehr, geht der Fokus auf sein Schiff, sonst den Planeten.
  Kollisionen der Kamera mit dem Planeten werden bewusst vernachlässigt.
- **Zeit:** Es gibt eine Spielzeit für alles. Angezeigt wird sie in Tagen
  des Heimatplaneten, die Tageslänge kommt aus seiner Rotation (`hud::day`,
  heute 6 h); keine Ortszeit, kein Kalender pro Körper, auch am Mond gilt
  diese Zeit (wie die Kerbin-Zeit in KSP). Der Tag ist der Sonnentag, aus
  Umdrehung und Umlauf um die Sonne berechnet (`body::day_length`); wie
  Kerbin dreht sich Zorb dafür in 5:59:09, damit der Sonnentag genau 6 h
  dauert.
- **Einflusssphären gestrichelt** (`view::draw_dashed`), damit sie nicht wie
  Bahnen aussehen: Strich 10 px, Lücke 7 px, abgemessen entlang der
  Projektion auf dem Bildschirm, also bei jedem Zoom und schräger Sicht
  gleich. Beim Zoomen ändert sich deshalb die Zahl der Striche.
- **Bahnen um den Mond zeichnen** wie in KSP (`view::anchors`):
  Aufeinanderfolgende Abschnitte um denselben Körper bilden ein Stück; es
  liegt um die Stelle, an der der Körper zu Beginn des Stücks steht
  (frühestens jetzt). So ist das Stück um den Mond eine echte
  Kegelschnittbahn um ihn, Periapsis und Vorbeiflug sind ablesbar; dort
  steht ein blasser Geistermond, kleine Ringe markieren Ein- und Austritt.
  Beim Austritt klafft eine Lücke (der Mond ist weitergezogen). Die
  Alternative, der echte Weg im System des Planeten, wäre durchgehend, um den
  Mond aber eine verzerrte Schleife. Schiffe, Nodes, Klicks und Kamera
  rechnen mit demselben Versatz; Oberflächenpunkte (Zielgebiete,
  Aufsetzkreuz) liegen auf dem echten Körper.
- **Nahansicht** (`N`, wie die Flugansicht in KSP): nur mit einem Objekt im
  Fokus; Zoom bis 10 m (in der Karte bis 20 km), "oben" zeigt vom Planeten
  weg. Bahnen werden dann mit 4096 statt 256 Punkten pro Umlauf gezeichnet
  und angeklickt (sonst liegt ein Schiff bis 50 m neben seiner Linie). Treffer
  auf Bahnen werden in 3D mit dem Strahl unter der Maus gesucht, damit auch
  Stücke zählen, deren Enden außerhalb des Bildes liegen. Die Planetenkugel
  ist auf etwa 6 m genau, Zielgebiete und Aufsetzkreuz liegen 2 m, das
  Gitter 10 m über dem Boden. Später kommen hier die Modelle der Schiffe hin.
- **Aufbau des Bildes:** Über der Karte steht kein Text außer Warnungen und
  Beschriftungen an ihren Punkten; alles andere liegt in Leisten.
  - *Leiste oben,* zwischen den Seitenleisten: Spielzeit, Zeitraffer,
    Kamera; ganz rechts die Schalter für die Fenster. Sie wird nach den
    Seitenleisten angelegt und beginnt deshalb von selbst an deren Kante.
  - *Δv-Balken* unten über die ganze Breite, die Seitenleisten stehen
    darüber.
  - *Seitenleisten* (`side.rs`, eine Bauart für beide): Kopfzeile mit dem
    Knopf zum Ein- und Ausklappen an der Außenkante (links dazu die Schalter
    Schiffe und Info), darunter der Inhalt, am unteren Ende der Regler für
    die Deckkraft über die ganze Breite. Die Karte läuft hinter den Leisten
    weiter; eingeklappt bleibt nur ein schmaler Streifen mit dem Knopf.
  - *Leiste links, reine Information* (ausblendbar): die Liste der Schiffe
    (Name und Körper; ein Klick wählt) und die Angaben zur Auswahl. Beim
    Schiff stehen die Flugdaten hervorgehoben: Apoapsis, Periapsis, Höhe,
    Geschwindigkeit auf der Bahn und zum Boden (senkrecht, waagerecht). Die
    Höhe zählt ab der Kugel des Körpers; solange es kein Gelände gibt, ist
    das zugleich die Höhe über dem Boden.
  - *Seitenleiste rechts, alles zum Bedienen,* als Karten je nach Lage:
    „Start“ bei einem gelandeten Schiff, „Manöver“ mit der Kette der Nodes,
    „Ziel“ bei jedem Schiff außer Wracks, dazu Verbund, Fracht & Basis und
    Startbasis.
  - *Warnungen* zum gewählten Schiff (Absturz, ungültige Nodes, zu wenig
    Treibstoff für einen Node, Andocken unmöglich) stehen rot über der
    Karte, damit sie auch bei ausgeblendeten Leisten zu sehen sind.
  - *Nichts doppelt:* Treibstoff und Δv zeigt nur der Δv-Balken, den
    geplanten Start nur die Karte „Start“, Annäherung und Andocken nur die
    Karte „Ziel“.
  - *Fenster* (Schalter oben rechts) für alles, was nicht an der Auswahl
    hängt: die Hilfe (auch `F1`), die Entwürfe (`entwurf.rs`, der
    Schiffseditor) und die Wirtschaft (`wirtschaft.rs`). Die Leiste oben legt
    zuerst die Schalter an und kürzt bei wenig Platz die Angabe zur Kamera. Die
    Wirtschaft ist reine Anzeige: je Basis Vorrat und Stand ("voll in …",
    aus dem geschlossen gerechneten Lager), dazu die Fracht an Bord. Sie
    zeigt bewusst keine Summe der Förderraten (ein volles Lager fördert
    nichts; der Durchsatz hängt an den Frachtern) und keine Preise (die
    stehen in der Karte „Startbasis“). Später tritt an die Stelle des
    Schalters die Anzeige der Daten (in GB): Klick öffnet das Fenster,
    Überfahren zeigt die Stoffe im Lager der Startbasis.
  - *Einheiten:* Lager und Fracht in Tonnen, der Tank eines Schiffs in kg.
  - *Zahlen:* überall mit Dezimalkomma und ab fünf Stellen mit Punkten
    zwischen den Tausendern (`zahl::De`: eine Hülle um die Zahl, die
    Genauigkeit kommt wie gewohnt aus dem Format).
- **Oberfläche mit egui** (`bevy_egui`; Entscheidung vor dem Schiffseditor,
  der Regler, Listen und Textfelder braucht). Die ganze Oberfläche ist in
  egui, daneben gibt es nur die 3D-Ansicht; Bevys eigenes UI ist nicht mehr
  im Build. Ein System (`ui::draw`) zeichnet alles: Statusleiste,
  die Leisten oben, links und rechts, Δv-Balken, Warnungen und die
  Beschriftungen an Punkten der Karte (die 3D-Ansicht trägt sie in `WorldLabels` ein). Sauberer
  Code ging beim Umzug vor gleichem Aussehen.
  - egui zeichnet jedes Bild neu: Eine Karte ist eine Funktion, die die Welt
    liest, zeichnet und bei einem Klick ändert (`start_card::show` und so
    weiter). Aufbau vorab, Abgleich der Texte und eigene Treffer-Prüfung
    entfallen.
  - Die Eingabe der 3D-Ansicht fragt egui, ob die Maus der Oberfläche gehört
    (`EguiWantsInput`), statt Rechtecke zu vergleichen.
  - Schrift ist dieselbe (Fira Mono, dicktengleich): Spalten in den Karten
    sind weiter aufgefüllter Text.
  - Preis: ein Fremdpaket, das bei einem Bevy-Update nachziehen muss; die
    Bevy-Version ist ohnehin fest.
- **Statusleiste mit Protokoll** ganz unten: letzte Meldung mit Spielzeit,
  ▲ klappt die letzten Meldungen auf (12 Zeilen, zum Scrollen; wer
  hochgescrollt hat, bleibt an seiner Stelle). Protokolliert
  werden Start, Brennbeginn, Brennschluss (Ap/Pe danach), Eintritt in die
  Atmosphäre (Pe), Landung (Zielgebiet) und Absturz (Grund). Gespeichert sind
  Daten, kein Text (`LogKind`); die Anzeige formuliert selbst. Höchstens 1000
  Einträge. Hinweise der Bedienung (gelb) stehen dazwischen, gehören aber
  nicht zum Spielstand.
- **Δv-Balken unten** für das gewählte Schiff: volle Breite = voller Tank
  (`VesselParams::fuel_capacity`), gefüllt = Δv an Bord. Darin die geplanten
  Brennvorgänge der Kette in Reihenfolge (Start gelb, Nodes abwechselnd
  violett, der gewählte hell umrandet), dann der Rest (grün). Verlangt die
  Kette mehr als an Bord ist, steht der Fehlbetrag rot dahinter (der Balken
  wird dafür länger). Teilstriche alle 500 m/s. Die Teile gehen exakt auf, weil
  sich bei fester Ausströmgeschwindigkeit das Δv der Brennvorgänge zum Δv des
  Tanks addiert (`World::delta_v_budget`).
- **Landung über den Manöver-Node:** Die Vorschau rechnet bis zum Boden.
  Aufsetzpunkt als Kreuz: grün im Zielgebiet, gelb außerhalb, rot bei
  Absturz. Zielgebiete als Kreise auf der Oberfläche (5 km Radius, aus der
  Ferne mindestens so groß wie ein Markierungspunkt). Fallschirme = größere
  Widerstandsfläche ab einer Auslösehöhe (dasselbe Modell). Das letzte Stück
  fliegt das Schiff selbst: ein automatisches Landebrennen (voller Schub,
  so spät wie möglich, wie ein „Suicide Burn“ in KSP), braucht Schub >
  Gewicht. Der Δv-Balken zeigt es als eigenen Abschnitt „Landen“.
- **Ziel verfehlt:** Mit genug Treibstoff setzt das Schiff trotzdem auf, steht
  dann „außerhalb“ und kommt mit einem „Hop" ins Ziel (= Startprofil mit
  niedriger Apoapsis, keine eigene Mechanik). Ohne Treibstoff oder Schub ein
  Wrack.
- **Satelliten** sind normale Schiffe. Zu tief ausgesetzt = Fehlschlag (Absturz).
- **Rendezvous:** Ziel pro Schiff (Karte „Ziel“, ◄ ►), engste Annäherung
  und Neigung zur Zielebene anzeigen, verbessert wird mit normalen
  Manöver-Nodes. Zielbahn rosa, Annäherung als zwei Ringe mit Linie, Knoten
  als Rauten „AN“/„DN“. Das Startfenster findet man von Hand: Startzeit und
  Kurs verschieben, bis die Neigung klein ist.
- **Andocken** als letzter Schritt der Kette (siehe Physikmodell): Schaltfläche
  in der Karte „Ziel“, Ablauf und Kosten dort, in der Kettenzeile links und
  rosa im Δv-Balken; der Angleich-Node ist ein doppelter rosa Ring ohne
  Griffe. Das Ziel wird Hauptschiff, der Verbund kann normal brennen (so
  lassen sich größere Schiffe im Orbit zusammensetzen). In der Liste steht er
  als „Frachter-1 + Frachter-3“, angedockte Schiffe sind nicht einzeln wählbar.
- **Abdocken** in der Karte „Verbund“: Teil wählen, Treibstoff zum
  Mitnehmen mit − / + (Vorschlag nach Tankgröße), Vorschau beider Schiffe,
  „Abdocken“. Planbares Abdocken als Schritt der Kette kommt mit den
  automatischen Routen.
- **Schiffe sind Werte:** Ein Entwurf (Chassis, Triebwerke, Module, Tank;
  siehe „Katalog“) ergibt die `VesselParams`. Fähigkeiten sind Felder, keine
  eigene Logik pro Klasse; die Simulation kennt nur diese Werte.

## Wirtschaft

Der Engpass ist immer der Transport, damit die Orbitalmechanik das Tempo
der Wirtschaft bestimmt. Grundlage ist Regel 14. Es gibt zwei Zähler: Zorit
(ein Stoff, bezahlt Schiffe und Bauteile) und Daten (schalten Technik frei).

**Der Kreislauf (`sim::base`):** bewusst klein, um den Spielspaß zu
prüfen. Leitgedanke: Der Engpass ist die Leistung der Schiffe. Bauteile sind
eine Investition (kein Verbrauch): Eine Basis erweitert dauerhaft, wie weit
man kommt und was man zurückbringt.

- **Kreislauf:** Bauteile von Zorb hinaus → Basen → Zorit zurück nach Zorb →
  daraus Schiffe und Bauteile → weiter hinaus. Hinaus ist
  teuer (Start von Zorb, voll beladen), zurück billig (schwache Schwerkraft,
  auf Zorb bremst die Luft).
- **Güter und Inventare** (`sim::goods`): Ein Gut ist eine Sorte Fracht
  (`Good`: Bauteile, Zorit, im Lager einer Tankstelle auch Treibstoff), ein
  Inventar eine Liste aus Gut und Menge. Es gibt es an zwei Stellen: als
  Frachtraum eines Schiffs (`Vessel::cargo`; alle Güter teilen ihn sich) und
  als Lager einer Basis (`Base::stock`). Dazwischen gibt es genau zwei
  Handlungen, Einladen und Ausladen (`World::load`, `World::unload`, mit
  Menge); die Basis bestimmt, was sie herausgibt und annimmt: die Startbasis
  alles, eine Baustelle nimmt Bauteile, eine Mine gibt Zorit. Ein neues Gut
  braucht damit nur einen Eintrag in `Good`, keinen eigenen Code zum Lagern
  und Laden. Was keine Lagerbewegung ist, bleibt eine eigene Handlung:
  tauschen, tanken, bestellen, umbauen.
- **Treibstoff** liegt im Schiff im Tank und ist dort kein Frachtgut, weil
  er zum Flug gehört (Masse, Δv). Fracht zählt
  als Masse (`World::params` rechnet sie zur Masse ohne Treibstoff; damit
  stimmen Brennen, Δv-Balken, Landung). Zorit ist ein seltenes Erz, das es
  auf Zorb nicht gibt; vorerst der einzige Rohstoff, weitere Arten kommen mit
  den Rohstoffkarten.
- **Eine Basis, ein Stoff** (`BaseKind`): Eine Tankstelle stellt Treibstoff
  her, eine Mine Zorit. Wer an einem Ort beides will, baut zwei Basen.
- **Lager der Startbasis:** unbegrenzt, für alle Güter. Mit dem Zorit darin
  bezahlt man (`World::zorit`); Treibstoff gibt sie frei aus.
- **Gründen:** Das Basisschiff gründet auf einem Körper außer dem
  Heimatplaneten eine Mine oder Tankstelle und wird ihr Fundament
  (`foundation_of`; ein Start ist dann gesperrt). Der Landepunkt wird zur
  Zone: ein Zielgebiet mit 5 km Radius, damit Karte, Landevorschau und
  Protokoll ohne Sonderfall damit umgehen. Im Umkreis einer Basis entsteht
  keine zweite.
- **Baustelle:** 4 t Bauteile. Schiffe, die im Umkreis stehen, laden ab (so
  viel sie haben und noch fehlt). Mit dem letzten Bauteil ist die Basis
  fertig.
- **Herstellung:** gleichmäßig, bis das Lager voll ist; vorerst aus dem
  Nichts, bis es Strom und Bewohner gibt. Gespeichert sind Menge und
  Zeitpunkt der letzten Änderung (`Base::stock`, `Base::since`), der Stand
  dazwischen wird direkt berechnet (`World::stock`; Regel 14, unabhängig
  von der Schrittgröße). Tankstelle: 180 kg
  pro Stunde, Lager 8 t. Mine: 100 kg pro Stunde.
- **Häufigkeit = Lagergröße:** Weil Vorspulen nichts kostet, ist jede Rate
  wirkungslos; auch eine schlecht gelegene Mine liefe voll. Knapp sind nur
  die Flüge, die Leistung der Schiffe und endliche Mengen. Die Häufigkeit von
  Zorit bestimmt deshalb, wie viel das Lager einer Mine fasst (`Body::zorit`:
  Zoon 1 t, Zill 2 t, Nura 4 t), also den Ertrag pro Abholung. Später ersetzt
  die Rohstoffkarte diese eine Zahl je Körper; reiche Stellen sollen dort
  liegen, wo das Hinkommen teuer ist. Endliche Vorkommen bleiben eine Option.
- **Die Startentwürfe passen dazu:** Der Frachter (2 t Frachtraum) startet voll beladen von Zorb, landet auf Zoon und kommt ohne
  Tanken mit Zorit zurück; das Basisschiff (`base_kit`) fliegt nur hin.
- Laden, Abladen und Tanken gehen nur am Boden, ohne geplanten Start und
  ohne angedockte Schiffe: Dann ändert sich nur die Masse des stehenden
  Schiffs, keine Bahn muss neu gerechnet werden.
- In der Demo kreisen schon zwei Basisschiffe und zwei Frachter mit
  zusammen 4 t Bauteilen um Zorb (mit dem Treibstoff, der nach einem Start
  übrig wäre).

**Preise und Bestellen:** Mit dem Zorit im Lager der Startbasis tauscht man
dort Bauteile ein (0,25 t je Tonne; `World::buy_parts`) und bestellt Schiffe nach Entwurf (die
Startentwürfe kosten je 1 t; `World::order_ship`). Eine neue Basis kostet damit 2 t
Zorit: zwei Abholungen auf Zoon, eine auf Zill. Treibstoff ist an der
Startbasis frei.
- Ein bestelltes Schiff steht sofort vollgetankt an der Startbasis: Eine
  Bauzeit wäre wirkungslos, weil Vorspulen nichts kostet.
- Tauschen und Laden sind zwei Schritte: Eingetauschte Bauteile liegen im
  Lager, bis ein Schiff sie lädt. Reicht das Zorit nicht für die ganze
  Menge, gibt es so viele Bauteile, wie es bezahlt. Zurücktauschen gibt es
  nicht.
- **Spielbeginn** (`scenario::start`): keine Schiffe, nur die Startbasis
  mit 3 t Zorit, genau dem Preis einer Basis (Basisschiff, ein Frachter,
  4 t Bauteile). Bewusst keine Absicherung gegen die Sackgasse: Wer damit
  keine Mine fertig bekommt, steckt fest; das ist die Gefahr des frühen
  Spiels.
- **Demo** (`scenario::demo`, im Spiel `--demo`): der Spielbeginn, dazu
  weitere Zielgebiete und Schiffe aus den Startentwürfen, die schon unterwegs
  sind (die Nummer im Namen ist die Nummer des Schiffs, der Booster ist
  Nummer 0). Sie ist die Grundlage der Beispiele im README und vieler Tests.
  Es gibt damit nur eine Welt: Was die Demo zeigt, geht im Spiel genauso.
- **Prüfschiffe** (`tests/common`): Tests, die ihre eigene kleine Welt
  bauen, nehmen dafür auch Werte, die der Katalog nicht hat (sehr wenig
  Schub, kein Fallschirm). Die Regeln der Simulation gelten für beliebige
  Werte; im Spiel und in der Demo gibt es diese Schiffe nicht.
- Im Spiel: Karte „Startbasis", sobald ein Schiff gewählt ist, das dort
  steht, oder ihr Zielgebiet (dessen Raute lässt sich nur anklicken, wenn
  kein Schiff darauf steht; Schiffe haben beim Klicken Vorrang).

**Reichweiten** (Δv in m/s ab niedriger Bahn um Zorb; Zoon und die Landung
auf Nura in der Simulation gemessen, der Rest gerechnet, ohne
Kurskorrekturen):

| Ziel | Hinflug | Landung | Summe | Rückflug bis in Zorbs Luft |
|---|---|---|---|---|
| Zoon | 845 | 910 | 1.760 | 1.000 |
| Zill | 900 bis 930 | 1.100 bis 1.200 | 2.100 bis 2.300 | 1.150 |
| Nura | 1.090 | 30 | 1.150 | 1.750 |

Im Orbit haben nach dem Start von Zorb (auf dem Booster, siehe unten) das
Basisschiff rund 2.150 und der Frachter mit 2 t Bauteilen rund 3.000 übrig;
mit vollem Tank rund 3.850 und 4.500. Teuer macht ein Ziel nicht die Entfernung,
sondern Landen ohne Luft in tiefer Schwerkraft. Daraus die Stufen: Zoon
geht direkt, Zill braucht eine Tankstelle (oder bessere Triebwerke).
Nura ist beim Hinflug am billigsten, weil Luft und Fallschirm die Landung
übernehmen (der Fallschirm hält im Modell bei jeder Geschwindigkeit); teuer
sind dort Rückflug, Startfenster und Flugzeit.

**Booster** (`sim::booster`): Jeder Start von der Startbasis läuft
als Stapel aus Schiff und Booster. Der Booster gehört zur Startbasis, nicht
zum Schiff; es gibt genau einen.

- *Warum:* Er trennt zwei Aufgaben. Der Booster hebt das Schiff aus Zorbs
  Schwerkraft und Luft, das Schiff ist für die Reise gebaut (halb so große
  Tanks wie davor). Daraus werden zwei Wege für Upgrades: bessere Booster
  tragen schwerere Schiffe, bessere Triebwerke geben Reichweite.
  Wirtschaftlich spart er vorerst nichts (Treibstoff an der Startbasis ist
  frei, Schiffe gehen nicht verloren).
- *Aufstieg:* Das Brennen beim Start trägt Schub, Verbrauch und Masse des
  Stapels (`Ascent::stack`); es zählt die größere Widerstandsfläche der
  beiden. Die Masse des Schiffs bleibt dabei gleich, und mit ihr fliegt es
  nach dem Brennschluss weiter. Der Aufstieg endet an der Ziel-Apoapsis oder
  wenn nur noch die Reserve im Tank ist: Der Booster hält immer Treibstoff
  für 1.200 m/s zurück (B-1: knapp 1,5 t): 1.000 für den Boostback, 200 für
  die Landung.
- *Rückflug:* Beim Planen des Starts wird der Boostback gesucht: ein Stoß
  waagerecht gegen den Kurs (dessen Richtung dort, wo der Booster ist; die
  Bewegung über dem Boden taugt nicht als Richtung, nach einem steilen
  Aufstieg ist sie fast null), 10 s nach der Trennung. Nach einem sehr
  steilen Aufstieg fiele der Booster hinter den Startplatz, weil sich der
  Planet unter ihm weiterdreht: Dann geht der Stoß nach vorn; trifft er
  ohnehin, gibt es keinen.
  Gesucht wird sein Betrag (Regula falsi; der Aufsetzpunkt wandert damit
  stetig entlang des Kurses zurück) und, weil die Drehung des Planeten den
  Booster je nach Kurs zur Seite trägt, ein Winkel dagegen
  (Sekantenverfahren). Bewertet wird jeder Versuch mit der echten Zeitleiste
  des Boosters samt Luft und Landebrennen. Bei der Trennung (ein Ereignis mit
  festem Zeitpunkt, wie der Kontakt beim Andocken) bekommt der Booster genau
  diesen Node: Vorschau = Ausführung, unabhängig von der Schrittgröße.
- *Kein Start,* wenn er nicht innerhalb von 2 km an der Startbasis aufsetzt,
  wenn er unterwegs ist oder für ein anderes Schiff verplant. Grenzen
  (gemessen mit dem B-1): Schiffe bis etwa 19 t; Kurse von Nord über Ost bis
  Süd gehen mit 10° Wendewinkel, nach Westen reicht die Reserve bei 100 km
  Ziel-Apoapsis nicht (Drehung des Planeten gegen sich).
- *Was das Schiff noch braucht:* rund 1.500 m/s bis in die Bahn (voller
  Frachter, 10° Wendewinkel), statt 3.600 aus eigener Kraft. Ein leichteres
  Schiff wird steiler geworfen und braucht mehr (Basisschiff: 1.740, mit 12°
  1.660); ein größerer Booster wirft kleine Schiffe ebenfalls steiler. Das
  Schiff braucht genug Schub: Bis zur Apoapsis sind es nur gut zwei Minuten.
- *Momentane Bahn in der Luft:* Während eines Brennens zeichnet die Karte
  die Bahn, die das Schiff hätte, wenn das Triebwerk jetzt abschaltete. Über
  der Luft ist das eine Keplerbahn, in der Luft der Freiflug aus derselben
  Zeitleiste wie überall (`Trajectory::new_flying`, in jedem Bild neu). So
  sieht man beim Boostback den Aufsetzpunkt zur Startbasis wandern, und bei
  einem Aufstieg aus eigener Kraft, wie hoch und wie weit es schon reicht.
  Solange Schiff und Booster gekoppelt sind, gibt es sie nicht: Brennschluss
  ist Trennung, und die Bahnen danach stehen für beide als geplante Bahnen
  da. Der Stapel rechnet ohne Fallschirm.
- *Landung ohne Fallschirm:* Die Luft bremst den fallenden Booster auf rund
  85 m/s, den Rest nimmt das automatische Landebrennen (`landing`) wenige
  Dutzend Meter über dem Boden, mit vollem Schub in unter einer Sekunde. So
  ist er gut fünf Minuten nach der Trennung zurück statt nach einer
  Viertelstunde am Schirm.
- *Bis zur Trennung* ist der Booster „am Schiff" (`VesselStatus::Carried`):
  Er hat keine eigene Zeitleiste und fliegt als Teil des Stapels mit. Eine
  Stelle in der Welt (`World::carried`) sagt für ihn, an welchem Schiff er
  hängt; daraus folgen Lage (die des Schiffs), Masse (der Stapel ohne das
  Schiff) und Status. Die Oberfläche fragt nicht nach „dem Booster", sondern
  nach diesem Status und danach, ob sich ein Fahrzeug steuern lässt
  (`World::steerable`). Seinen Δv-Haushalt vor der Trennung gibt es aus dem
  Plan des Starts (Aufstieg und Boostback, der Rest bleibt für die Landung),
  seinen Rückflug zeichnet die Karte bis dahin als Vorschau
  (`World::booster_preview`, neu gerechnet nur, wenn sich der Start ändert);
  bei der Trennung übernimmt er genau diese Bahn. An der Startbasis gelandet
  ist er sofort wieder vollgetankt.
- *Die Werte der Booster* (B-1, B-2) stehen in `sim::booster`, die der
  Schiffsteile in `sim::design`.

**Katalog** (`sim::design`): Chassis, Triebwerke und Module, daraus
Entwürfe mit ihren Flugwerten und Preisen. Der Editor (`entwurf.rs`, Fenster
„Entwürfe") ändert die Entwürfe der Welt direkt; einstellen lässt sich nur,
was sich bauen lässt, alles Weitere sind Warnungen. Was nach dem Start im
Orbit bleibt, kommt aus einer echten Probeplanung (`World::launch_estimate`:
dieselbe Startplanung in einer eigenen kleinen Welt, neu nur, wenn sich der
Entwurf ändert). Solange jemand in ein Textfeld tippt, laufen die Systeme
mit Tasten nicht (`ui::not_typing`). Jedes bestellte Schiff merkt sich seinen
Entwurf (`Vessel::design`); was man bauen darf, sagt `World::unlocked`
(zu Beginn die Ausstattung vom Spielbeginn, Weiteres über die Technik). Nur der Booster hat
feste Werte ohne Entwurf; auch die Schiffe der Demo kommen aus den
Startentwürfen.

| Chassis | Masse | Innenraum | Triebwerksplätze | Modulplätze | Preis | frei |
|---|---|---|---|---|---|---|
| Klein | 1.200 kg | 7 t | 1 | 2 | 0,4 t | von Anfang an |
| Mittel | 1.900 kg | 16 t | 2 | 4 | 0,6 t | von Anfang an |
| Groß | 3.800 kg | 32 t | 4 | 8 | 1,2 t | 30 GB (voll nur auf dem B-2) |

| Modul | Masse | Modulplätze | Preis | Wirkung | frei |
|---|---|---|---|---|---|
| Fundament | 1.500 kg | 2 | 0,4 t | gründet eine Basis | von Anfang an |
| Probenbohrer | 300 kg | 1 | 0,3 t | nimmt Proben vom Boden | 20 GB |

| Triebwerk | Schub | Ausströmgeschw. | Masse | Preis | frei |
|---|---|---|---|---|---|
| Z-1 | 250 kN | 3.200 m/s | 300 kg | 0,2 t | von Anfang an |
| Z-2 | 250 kN | 3.600 m/s | 350 kg | 0,4 t | 30 GB |
| Z-3 | 250 kN | 4.000 m/s | 400 kg | 0,8 t | 100 GB, braucht Z-2 |

| Booster | Leermasse | Treibstoff | Schub | trägt | frei |
|---|---|---|---|---|---|
| B-1 | 3 t | 30 t | 760 kN | etwa 19 t | von Anfang an |
| B-2 | 6 t | 60 t | 1.520 kN | etwa 38 t (37 t gemessen) | 50 GB, 3 t Zorit |

Der Innenraum gehört Tank und Fracht und teilt sich per Regler
(0,5-t-Schritte, nur an der Startbasis änderbar). Ein Schiff trägt nur
eine Sorte Triebwerk.

*Module* (`Module`) geben dem Schiff Fähigkeiten. Sie belegen Modulplätze
des Chassis und haben Masse und Preis, brauchen aber keinen Innenraum. Der
Entwurf führt sie als Liste (`Design::modules`), die Flugwerte als Menge
(`VesselParams::modules`); Regeln fragen nur „hat das Schiff Modul X?"
(`Modules::has`), im Verbund zählen die Module aller Schiffe. Ein neues
Modul ist damit ein Eintrag im Katalog und die Regel, die es nutzt; der
Editor zeigt es von selbst. Der Katalogeintrag sagt auch, wie oft ein Modul
in ein Schiff passt (bisher jedes einmal). Module mit Zahlenwerten
(Kapazität einer Batterie, Reichweite einer Antenne) bekommen diese Werte,
wenn das System dazu gebaut wird. Triebwerke bleiben eine eigene Angabe
(Sorte und Anzahl, eigene Plätze), weil sie die Flugwerte bestimmen. Das
kleine Chassis hat 7 t Innenraum, seit das Fundament keinen mehr belegt:
So bleibt das Basisschiff, wie es abgestimmt ist. Startentwürfe: Frachter = Mittel + 2 × Z-1, 14 t Tank,
2 t Fracht; Basisschiff = Klein + Z-1 + Fundament, 7 t Tank; beide 1 t Zorit,
beide löschbar. Ein Basisschiff mit freiem Innenraum darf Fracht tragen.

*Umbau* (`World::refit`): Ein Schiff, das bereit an der Startbasis steht
(wie beim Laden: kein Start geplant, nichts angedockt), bekommt einen
anderen Entwurf mit demselben Chassis. Bezahlt wird der Preisunterschied,
ein billigerer Entwurf bringt ihn voll zurück; so kostet Ausprobieren
nichts, und ein anderes Chassis bleibt ein neues Schiff. Auch das Fundament
ist ein Teil wie jedes andere. Das Schiff steht danach vollgetankt da
(Treibstoff ist dort frei) und behält Namen und Fracht; passt die Fracht
nicht in den neuen Frachtraum, wird verweigert. Es geschieht sofort
(Regel 14). Der Booster hat keinen Entwurf und wird nur über die Technik
ersetzt.

**Daten und Aufgaben** (`sim::task`): Daten (GB) sind der zweite
Zähler (bestimmen, *was* man bauen kann; Zorit, *wie viel*). Sie sind
keine Fracht und gehören keinem Ort: Sie werden gefunkt und sofort
gutgeschrieben.

- *Ein System für alles:* Eine Aufgabe ist Körper, Ziel und Belohnung. Ziele
  sind eine Umlaufbahn nach Vorgabe, eine Landung oder eine fertige Basis.
  Die feste Liste leitet sich aus den Körpern ab (Grundwert `Body::data`:
  Zorb 5, Zoon 5, Zill 10, Nura 15; niedrige und hohe Umlaufbahn einfach,
  polare und Landung doppelt, Basis dreifach; zusammen 290 GB).
  Storymissionen und erzeugte Aufträge sind später weitere Einträge derselben
  Art.
- *Orbitvorgaben sind Bereiche* für Periapsis, Apoapsis und Bahnneigung. Die
  Breite des Bereichs ist die Toleranz. Die Grenzen der festen Aufgaben
  folgen aus Radius und Luftgrenze des Körpers (niedrig: bis ein Viertel
  Radius über Luft oder Boden; hoch: ab zwei Radien).
- *Gezählt wird an Ereignissen des Protokolls* (Brennschluss, Landung, Basis
  fertig): Jeder Eintrag wird beim Eintragen geprüft, der Eintrag
  „Aufgabe erfüllt" steht direkt dahinter. Der Brennschluss trägt dafür
  Körper und Bahnneigung. So hängt nichts von der Schrittgröße ab, und es
  zählt nur, was ein Schiff wirklich tut: Vorbeiflüge und Schiffe, die schon
  auf einer Bahn stehen, zählen nicht. Der Booster erfüllt nichts.
- *Vorschau statt Fummeln:* Die Karte „Manöver" zeigt beim Node, welche
  offenen Aufgaben die Bahn danach erfüllt (`World::tasks_met_by`, dieselbe
  Prüfung wie beim Zählen).
- *Proben* (`Good::Sample`, je Körper ein eigenes Gut): Ein Schiff mit
  Probenbohrer lädt sie vom Boden des Körpers, auf dem es steht (nicht auf
  Zorb); der Boden ist unbegrenzt, der Frachtraum nicht, und auf Zoon
  konkurrieren sie mit dem Zorit der Mine um denselben Platz. An der
  Startbasis abgeladen, bringen sie Daten: je Tonne das Doppelte des
  Grundwerts (Zoon 10, Zill 20, Nura 30 GB), bis 2 t je Körper, zusammen
  120 GB. Die volle Menge bringt damit doppelt so viel wie eine Landung,
  weil man sie auch zurückbringen muss. Die abgegebenen Proben bleiben im
  Lager der Startbasis und kommen nicht wieder heraus: Das Lager ist die
  Buchführung (`World::sample_data`), einen eigenen Zähler gibt es nicht.
  Überall sonst kippt man Proben einfach aus. Tragen und abgeben kann sie
  jedes Schiff mit Frachtraum.
- *Techtree* (`sim::tech`; eine Liste mit Voraussetzungen):
  Probenbohrer (20 GB, so viel wie die drei Umlaufbahnen um Zorb bringen),
  Z-2 (30 GB), Chassis Groß (30 GB), Booster B-2 (50 GB und 3 t Zorit), Z-3
  (100 GB, braucht Z-2); zusammen 230 von 410 GB. Freischalten geschieht
  sofort (eine Forschungszeit wäre wirkungslos, Regel 14). Gespeichert
  sind nur die Technologien (`World::techs`); welche Teile baubar sind,
  leitet `World::unlocked` daraus ab. Der B-2 ersetzt die Werte des Boosters an der
  Startbasis, endgültig und nur, wenn er dort frei steht. Gemessen trägt er
  ein volles großes Chassis (37 t) und kommt zurück. Das große Chassis hat
  keine Voraussetzung: Mit kleinem Tank fliegt es auch auf dem B-1, der
  Editor warnt sonst. Was freigeschaltet wird, meldet die Statusleiste als
  Hinweis; im Protokoll der Simulation steht es nicht, das gehört den
  Schiffen.
- *Fenster:* Daten verdienen und ausgeben liegen an einer Stelle, hinter
  „Daten … GB" oben: Reiter Übersicht, Aufgaben, Technik.

## Stand und nächste Schritte

Gebaut und spielbar: Fliegen (Manöverketten, Start auf dem Booster, Landung,
Rendezvous, Andocken, Flüge zu Monden und Planeten), der Kreislauf aus
Bauteilen, Basen und Zorit, Entwürfe aus dem Katalog, Aufgaben für Daten und
die Technik, dazu der Umbau gebauter Schiffe, Module und Proben.

Vereinbart, noch nicht gebaut:

- **Erzeugte Aufgaben** mit ungewöhnlichen Bahnen und kleiner Toleranz: Der
  Zufall müsste in den Spielstand, und Daten wären nicht mehr endlich. Das
  entscheidet sich mit der Story-Kampagne; die Preise der Technik richten
  sich nach der Menge an Daten, die es dann gibt.

# Ideen (nicht gebaut)

Gesammelt, damit nichts verloren geht. Was hiervon gebaut wird, wandert nach
oben.

## Wirtschaft

Vorbilder sind Aufbauspiele wie die Anno-Reihe.

**Aufbau wie bei den Bahnen** (gilt schon für das Lager einer Basis,
`World::stock`):
Die Wirtschaft einer Basis ist eine Folge von
Abschnitten zwischen Ereignissen (ein Gebäude wird fertig, ein Lager ist
voll oder leer, ein Schiff kommt an, Bewohner ziehen ein oder aus). Jeder
Abschnitt speichert seinen Zustand zu Beginn; alles dazwischen (Lagerstände,
Batterie, geförderte Menge) wird für jeden Zeitpunkt direkt berechnet, nie
Schritt für Schritt. Ein Ereignis beendet den Abschnitt, der nächste beginnt
mit dem Zustand, der gerade herrscht: kein Sprung, nichts wird rückwirkend
neu berechnet. So gelten die Regeln 3, 5, 6 und 12 auch hier (keine Drift,
gleich bei jedem Zeitraffer, unabhängig von der Schrittgröße, Vorschau =
Ausführung).

**Zorb und die Daten:**
- Zorb ist nicht ausbaubar: Startplatz und Partner, der Aufbau findet im All
  statt.
- Daten in zwei Stufen: **gefunkte Daten** (Scans, Messungen;
  sofort da, wenig wert, später abhängig vom Kommunikationsnetz) und
  **Proben**, die man zurückbringen und heil landen muss (viel wert). So gibt
  es Fracht in beide Richtungen.
- Jede Entdeckung zählt nur einmal. Daten lassen sich also nicht durch
  Wiederholen vermehren; die Karten ganzer Körper zu scannen liefert viele
  kleine Portionen.
- **Aufträge** geben Richtung und Bonus („scanne Nura: doppelt“). Es gibt
  eine feste Zahl offener Aufträge; ein neuer erscheint, wenn einer erledigt
  ist, nicht mit der Zeit.
- **Ziel ist die völlige Unabhängigkeit:** Am Ende stellt man alles selbst
  her und braucht Zorb nur noch für Zorbels.
- **Endlos danach:** Labore (spätes Gebäude) machen aus hochwertigen Gütern
  Daten, pro Arbeit; damit wiederholbare Upgrades, jedes Mal teurer. Sie
  dürfen nicht zu früh kommen, sonst entwerten sie das Erkunden. Als
  Erweiterung: neue Ziele (Asteroiden, Kometen), die erscheinen, sobald eines
  erkundet ist.

**Rohstoffe und Landezonen:**
- Pro Körper und Rohstoff eine Karte wie eine Höhenkarte, aus einem Startwert
  erzeugt (gespeichert wird nur der Startwert).
- Scans decken sie auf: Ein Satellit sieht die Streifen, die er überfliegt
  (die Bahnneigung entscheidet, was er je zu sehen bekommt). Das gibt
  Satelliten einen Zweck und liefert Forschungsdaten.
- Eine Zone wird nicht vorher gewählt: **Der Landepunkt wird zur Zone.** Ihr
  Ertrag ist die Summe der Karten in ihrem Einzugsradius.
- Wie viele Zonen ein Körper trägt, folgt aus seiner Größe (Mindestabstand
  zwischen Zonen). Vorkommen erschöpfen sich nicht.
- Körper werden nicht als Ganzes beansprucht; „erschlossen“ ist, wo Basen
  stehen. Die festen Zielgebiete der Demo werden dadurch abgelöst.

**Basis:** entsteht, sobald an einer Zone ein Kontingent an Bauteilen
abgeladen ist. Gebäude: Mine, Fabrik, Raffinerie, Kraftwerk, Wohnraum, Lager,
Raumhafen. Lager haben Obergrenzen; ist eines voll, steht die Produktion
davor still.

**Bewohner (Zorbels, kleine Roboterwesen):**
- Auf Zorb gibt es beliebig viele. In die Startbasis ziehen so viele, wie
  Wohnraum und Versorgung tragen; das stellt sich schnell ein und hängt nicht
  an der Zeit.
- Überall sonst gibt es nur die, die man hinbringt: Sie reisen als Passagiere
  in einem Wohnmodul, in jede Richtung (auch umsiedeln). Sie vermehren sich
  nicht.
- Sie arbeiten (Gebäude brauchen Bewohner) und verbrauchen dabei.
- Bedürfnisse: **Strom** (Grundbedürfnis) und **Ersatzteile** (Verschleiß,
  pro Arbeit, nicht pro Tag). Nahrung, Wasser und Luft entfallen. Höhere
  Bedürfnisse („Upgrades“ mit seltenen Stoffen) erst später.
- Fehlt die Versorgung, stehen sie still (mild): Sie arbeiten nicht, bleiben
  aber, und nichts geht kaputt.

**Strom als Leistungsbilanz:** Erzeuger liefern Leistung (kW): Solarpanele
nur im Licht und je nach Abstand zur Sonne, ein Reaktor immer (teuer im Bau,
kein Brennstoff pro Zeit). Verbraucher ziehen Leistung. Eine Batterie (kWh)
ist ein Puffer: Sie nimmt Überschuss auf und gibt ihn bei Mangel ab, bis sie
leer ist; dann stehen die Verbraucher, bis wieder Licht ist. Weil sich Licht
und Dunkel wiederholen, ist der Füllstand für jeden Zeitpunkt direkt
berechenbar, und man sieht bei langsamem Zeitraffer, wie sie lädt und
entlädt. Nach einer Änderung (z. B. mehr Panele mitten am Tag) beginnt ein
neuer Abschnitt mit dem Füllstand von eben; der erste Tag ist ein Übergang,
spätestens nach dem ersten „voll“ oder „leer“ wiederholt sich der Verlauf
(Sonderfall zum Testen: Erzeugung und Verbrauch genau gleich, dann bleibt der
Startwert als Versatz). Produktion und Verschleiß folgen dem echten Verlauf
(nachts Stillstand), nicht einem Mittelwert. Die Standorte unterscheiden sich
damit von selbst: Zorb 3 h Nacht, Zoon gut 19 h, Nura etwa 43 % der
Sonnenleistung. Dasselbe Modell gilt für Schiffe (Wohnmodule brauchen
Leistung, keinen Vorrat für die Flugdauer); Energiezellen als Frachtgut gibt
es deshalb vorerst nicht.

**Erste Kette (im All):** Die erste Basis entsteht naheliegend auf Zoon: Mine (Erz) → Fabrik (Bauteile, Ersatzteile), dazu eine
Raffinerie (Treibstoff). Von Zoon ist der Weg in den Orbit viel billiger als
von Zorb (etwa 670 statt 3.400 m/s). Zorb gibt den Anfang, Zoon macht das
Fliegen billig, Nura liefert das Seltene.

**Weitere Module** (gebaut sind Modulplätze und das Fundament, siehe
„Katalog“): Scanner, Antenne, Wohnmodul, Dockingport, Solarpanel, Batterie,
Reaktor, Radiator …. Module liefern Werte, keine Geometrie (Regel 7); die Form hat keinen
Einfluss auf die Simulation. Module kommen in der Reihenfolge, in der ihre
Wirkung existiert. Zuerst als Liste; ein 3D-Editor zum Zusammenstellen ist
reine Darstellung und kommt später. Größere Schiffe entstehen als Rohbau im
Orbit, dem Teile angeliefert werden (wie das Kontingent einer Basis); der
Verbund aus angedockten Schiffen bleibt daneben bestehen.

**Später:** Sammler, die Gas beim Flug durch eine Atmosphäre ernten (mit der
Wärme eine Abwägung), Asteroiden als Körper, automatische Routen (früh
nötig, sobald Bewohner laufend versorgt werden), Kommunikationsnetz.

**Offen:**
- Stoffliste und Raten im Einzelnen; Bauteile und Ersatzteile: ein Gut oder
  zwei?
- Auftragsarten und ihre Steigerung: Sie müssen ohne Zeitdruck antreiben.
- Umladen im Verbund (im Orbit).

## Merkliste

- Kommunikationsnetz (Verbindung, Reichweite)
- Scans, automatisierte Routen, Rohstoffkarten
- Bergung von Wracks (über Daten)
- **Story-Kampagne** mit Storymissionen. Passt zu den Aufträgen: Eine
  Storymission ist ein Auftrag mit fester Reihenfolge und Text, kein zweites
  System. Aufhänger: warum Zorb Zorit und die anderen seltenen Stoffe
  braucht.
- „Hop zum Ziel": Parameter automatisch suchen, falls Zielen per Hand zu fummelig ist
- Modelle der Schiffe in der Nahansicht (Ausrichtung beim Anflug aus der
  Anfluglinie, Haltepunkt vor dem Andockstutzen)
- **Geparkt:** Bandbreite, Modulslots, Gelände/Höhenkarte

## Haushalte (alle nach demselben Muster)

Ein Vorrat pro Schiff, den jeder Abschnitt zu Beginn speichert (wie heute die
Masse); dazwischen ändert er sich nach einer Formel oder wird mit der Bahn
vorausberechnet. Quellen füllen ihn, Verbraucher leeren ihn, an einer Grenze
passiert etwas. So bleiben Vorschau = Ausführung, keine Drift und die
Unabhängigkeit von der Schrittgröße erhalten. Die Werte kommen aus Chassis
und Modulen.

- **Wärme** (mit Chassis und Modulen, weil es erst dann Gegenmaßnahmen gibt):
  Temperatur als Vorrat. Im Freiflug gibt das Schiff über Radiatoren Wärme ab;
  die Temperatur fällt nach einer Formel zur Gleichgewichtstemperatur hin
  (direkt für jeden Zeitpunkt). In der Luft nimmt es Wärme aus Luftdichte und
  Geschwindigkeit auf; das wird mit der Bahn vorausberechnet. Über der
  Grenztemperatur: „verglüht“, die Zeitleiste endet dort (wie beim Aufprall).
  Werte: Wärmekapazität, Grenztemperatur, Radiatoren, Hitzeschild (Anteil der
  Wärme, der ankommt). Damit wird Aerobraking über mehrere Umläufe eine echte
  Abwägung. Die Vorschau zeigt die Spitzentemperatur gegen die Grenze.
- **Strom** (mit dem ersten Verbraucher: Kommunikationsnetz oder Scans; als
  Leistungsbilanz mit der Batterie als Puffer, siehe „Wirtschaft“):
  Batterieladung als Vorrat. Quellen: Solarzellen (nur im Licht),
  Radioisotopen-Generator (immer gleich), Brennstoffzellen (verbrauchen
  Reaktanten, erst mit Rohstoffen). Verbraucher: Grundlast, Funk, Instrumente,
  später Ionentriebwerk oder aktive Kühlung (dann gekoppelt mit Wärme). Licht
  und Schatten wechseln jeden Umlauf: Die Schattenzeiten einer Keplerbahn
  lassen sich ausrechnen, die Ladung ist dazwischen stückweise linear
  (begrenzt bei voll und leer) und wird nach wenigen Umläufen periodisch, ist
  also für jeden Zeitpunkt direkt berechenbar, ohne neue Abschnitte. Ist sie
  leer, fallen zuerst nur Dienste aus (Funk, Scans, Aufträge); der Flug bleibt
  auf Schienen, Manöver laufen weiter. Ob ein Schiff ohne Strom auch nicht
  mehr steuern kann, wird später bewusst entschieden.
- **Sonne** (Zunn): ist in der Simulation (Wurzel der Körper, Flüge zwischen
  den Planeten). Für die Haushalte fehlen noch Schatten, Solarertrag und
  Gleichgewichtstemperatur; Abstand und Richtung zur Sonne gibt es schon
  (`body_state`).
- **Gestrichen:** Technikalterung, Lagrange-Punkte (mit Patched Conics nicht möglich)

## UI-Ideen

- **Scharf schalten:** Schaltfläche dafür in der Karte „Manöver“. Die
  Mechanik steckt schon in der Simulation; bis dahin ist jede Kette sofort
  scharf.
- **„+1 Umlauf“ / „−1 Umlauf“** für den gewählten Node in der Karte
  „Manöver“. Damit kommt ein Node auch auf einer stabilen Bahn auf einen
  späteren Umlauf.
- **Startfenster automatisch:** Startzeit und Kurs suchen, bei denen die
  Bahnebene zum Ziel passt (von Hand geht es über die Neigung in der Karte
  „Ziel“).
- **Abflugwinkel und Transferplaner:** wo auf der Parkbahn man für einen
  Flug zu einem anderen Planeten zünden muss, oder gleich ein fertiger Node
  (bewusst weggelassen: Man schiebt den Node, bis die Annäherung an den
  Zielkörper klein wird).
