# Kepler Industries

*Arbeitstitel.* Eine Wirtschaftssimulation in der Kartenansicht: Bahnen planen,
Fracht transportieren, Satellitennetze aufbauen, Basen gründen und alles am
Laufen halten. Geschrieben in Rust mit [Bevy](https://bevy.org).

Warum der Code so aufgebaut ist, steht in [`docs/ARCHITEKTUR.md`](docs/ARCHITEKTUR.md).

## Starten

Voraussetzung: Rust ≥ 1.97.1 (`rustup update`). Unter Linux die
[Systempakete für Bevy](https://github.com/bevyengine/bevy/blob/main/docs/linux_dependencies.md)
(Audio- und Gamepad-Pakete werden nicht gebraucht).

```sh
cargo run -p game
```

Der erste Build dauert einige Minuten, danach geht es schnell.

## Spielanleitung

Die Sonne Zunn mit zwei Planeten: Zorb, Heimat der Zorbels, mit Atmosphäre
und den Monden Zoon und Zill, weiter draußen Nura.

### Spielbeginn

Es gibt nur die Startbasis auf Zorb mit ihrem Booster, 3 t
Zorit im Lager und keine Schiffe. Der Vorrat reicht genau für eine Basis: ein Basisschiff,
ein Frachter und 4 t Bauteile (der Frachter fliegt dafür zweimal). Bestellt
wird in der Karte „Startbasis" der Seitenleiste nach Entwürfen; sie ist zu
Beginn offen. Ein Entwurf ist ein Chassis mit Triebwerken, optional dem
Fundament einer Basis und einer Aufteilung des Innenraums in Tank und
Fracht; daraus ergeben sich Flugwerte und Preis. Zu Beginn gibt es die
Entwürfe „Basisschiff" und „Frachter".
Eine Absicherung gibt es nicht: Wer mit dem Vorrat keine Mine fertig bekommt
(etwa als Erstes eine Tankstelle baut oder ein Schiff verliert), steckt fest.

### Schiffe und Entwürfe

Das Fenster „Entwürfe" (Schalter oben rechts) ist der Schiffseditor. Links
die Liste der Entwürfe: einen wählen, mit „Neu (Kopie)" einen weiteren anlegen, mit „Löschen" entfernen
(auch die beiden vom Beginn). Rechts der gewählte Entwurf: Name, Chassis,
Triebwerkssorte und Anzahl, Fundament und ein Regler, der den freien
Innenraum in Tank und Fracht teilt (0,5-t-Schritte). Darunter stehen Preis,
Massen, Schub, Δv und was nach einem Start auf dem Booster mit dem
Standardprofil im Orbit übrig bleibt, jeweils leer und voll beladen (dafür
wird der Start wirklich probeweise geplant). Was nicht taugt, steht als
Warnung da (kein Triebwerk, kein Tank, zu schwer für den Booster, reicht
nicht bis in die Umlaufbahn); bestellen lässt es sich trotzdem. Gesperrte
Teile (Groß, Z-2, Z-3) sind sichtbar, aber nicht wählbar, bis sie in der
Technik freigeschaltet sind. Änderungen gelten für künftige Bestellungen;
gebaute Schiffe bleiben, wie sie sind. Die Entwürfe gehören zum Spielstand.

**Umbau:** Ein Schiff, das an der Startbasis steht, lässt sich auf einen
anderen Entwurf mit demselben Chassis umbauen: andere Triebwerke, Fundament
ein- oder ausbauen, Tank und Fracht anders teilen. Die Karte „Startbasis"
zeigt dafür unter den Bestellungen eine Schaltfläche je passendem Entwurf mit
dem Preisunterschied; ein billigerer Entwurf bringt Zorit zurück. Das Schiff
steht danach vollgetankt da und behält Namen und Fracht. Passt die Fracht
nicht in den neuen Frachtraum oder ist ein Start geplant, geht es nicht.

### Start und Booster

Gestartet wird mit einem Startprofil aus vier Werten: Kurs, Wendebeginn,
Wendewinkel (so weit neigt sich das Schiff beim Wendebeginn aus der
Senkrechten) und Ziel-Apoapsis. Eingestellt wird es in der Karte „Start“ der Seitenleiste
rechts, die erscheint, sobald ein gelandetes Schiff gewählt ist. Die
Voreinstellung hängt vom Körper ab: mit Luft (Zorb) Wende ab 1,5 km mit 10°,
Ziel 100 km; ohne Luft (Zoon) ab 100 m mit 70°, Ziel 30 km. Die Vorschau zeigt den ganzen
Aufstieg (gelb) und die Bahn danach. Den Node zum Zirkularisieren an der
Apoapsis kann man schon vor dem Abheben setzen.

Jeder Start von der Startbasis läuft mit dem Booster. Schiff und
Booster heben als Stapel ab, nur der Booster brennt; das Startprofil in der
Karte „Start" gilt für den Stapel, das Schiff verbraucht dabei nichts. Bei
Brennschluss trennen sie sich: Das Schiff fliegt allein weiter und bringt
sich mit einem Node an der Apoapsis selbst in die Umlaufbahn (rund 1.500 m/s,
den Node setzt man vorab auf der Vorschau; bis zur Apoapsis sind es nur gut
zwei Minuten). Der Booster schiebt sich von selbst zurück und landet gut
fünf Minuten später an der Startbasis: ohne Fallschirm, mit einem kurzen
Landebrennen dicht über dem Boden. Steuern lässt er sich nicht. Die Karte „Start" zeigt vorab seinen Boostback und wann er zurück ist, die
Karte der Welt seinen Rückflug als geplante Bahn.
Es gibt nur diesen einen Booster. Kein Start, solange er unterwegs oder für
ein anderes Schiff verplant ist, und keiner, wenn er nicht zurückkäme: mit
einem zu schweren Schiff (er trägt etwa 19 t, ein voller Frachter wiegt
18,5 t) oder bei zu flachem Aufstieg (Wendewinkel kleiner wählen). Ein
leichtes Schiff wirft er steiler nach oben; es braucht dann etwas mehr
eigenes Δv, oder man wählt einen größeren Wendewinkel. Überall sonst, auch
auf Zorb außerhalb der Startbasis, starten Schiffe aus eigener Kraft. Die
Schiffe sind dafür klein: Der Frachter fasst 14 t Treibstoff, das Basisschiff
7 t.

### Manöver

Manöver plant man mit dem Manöver-Node wie in KSP, auch mehrere
hintereinander als Kette: Jeder Node liegt auf der Bahn nach den vorherigen
und wandert mit, wenn sich ein früherer ändert. Eine Kette ist sofort scharf:
Das Schiff brennt zu den geplanten Zeitpunkten selbst, auch unter Zeitraffer.
Ein Brennen beginnt nie vor jetzt und nie vor dem Ende des vorherigen, sonst
rückt der Node einen Umlauf weiter. Ein Node, für den es nach einer Änderung
keinen Platz mehr gibt (z. B. weil das Schiff vorher abstürzt), bleibt rot in
der Liste stehen, bis es wieder passt. Gelbe Ringe auf der Bahn zeigen Beginn
und Ende jedes Brennens. Während des Brennens zeigt die blaue Linie die momentane Bahn, die sich
sichtbar zur geplanten (violetten) hin verformt. Endet eine Bahn am Boden,
zeigt ein rotes Kreuz den Aufschlagpunkt.

Unten zeigt ein Balken den Tank des gewählten Schiffs als Δv: Die volle Breite
ist der volle Tank, darin die geplanten Brennvorgänge der Kette (Start gelb,
Nodes violett), dahinter der Rest (grün). Reicht der Treibstoff nicht, steht
rot dahinter, wie viel fehlt.

### Landen

Gelandet wird über einen Node, der die Bahn in die Luft führt. Die Vorschau
rechnet bis zum Boden, mit Fallschirm (unter 3 km). Ein Kreuz zeigt, wo das
Schiff aufsetzt: grün im Zielgebiet (grüne Kreise), gelb außerhalb, rot bei
einem Absturz. Das letzte Stück fliegt das Schiff selbst: Käme es schneller
als 5 m/s am Boden an, zündet es vorher automatisch ein Landebrennen (gelb auf
der Bahn, „Landen“ im Δv-Balken), voller Schub gegen die Bewegung zum Boden,
so spät, dass es knapp über dem Boden zum Stehen kommt. Das braucht Schub
größer als das Gewicht und genug Treibstoff; sonst wird gar nicht gebremst und
das Schiff stürzt ab (Beispiel „Absturz“ unter „Für Entwickler“). Die
Anzeige nennt dann Ort (auch das Zielgebiet) und Grund. Wer daneben landet, kommt mit einem Hop ins Ziel: einem Start aus
eigener Kraft mit niedriger Ziel-Apoapsis, zum Beispiel mit vollem Tank vom
Übungsfeld 60 km östlich zurück zur Startbasis (Kurs 270°, Wendewinkel 20°,
Ziel-Apoapsis etwa 49 km).

### Rendezvous und Andocken

Für ein Rendezvous wählt man in der Karte „Ziel“ der Seitenleiste ein
anderes Schiff (◄ ►). Dessen Bahn wird rosa. Die Karte zeigt den Abstand,
die engste Annäherung (wann, wie nah, wie schnell relativ zueinander) und die
Neigung zur Bahnebene des Ziels. Gesucht wird bis vier Umläufe nach dem
letzten geplanten Manöver, also auch auf den geplanten Bahnen. Auf der Karte
markieren zwei Ringe mit einer Linie die Annäherung, Rauten mit „AN“/„DN“
die Knoten, an denen ein Node die Bahnebene angleicht. Liegen beide Bahnen in
derselben Ebene (wie nach dem Abdocken), steht dort „gleiche Ebene“, und es
gibt keine Knoten. Am Boden gilt das für
die Bahn nach dem geplanten Start: Startzeit und Kurs so lange verschieben,
bis die Neigung klein ist, ergibt das Startfenster.

Liegt die engste Annäherung unter 1 km, dockt man mit „Andocken bei
Annäherung“ in der Karte „Ziel“ an. Die Kette bekommt dann einen
automatischen Node „Angleichen“ (doppelter rosa Ring): Er hebt an der engsten
Stelle die Relativgeschwindigkeit auf. Danach fliegt das Schiff auf gerader
Linie langsam ans Ziel heran (höchstens 2 m/s, Kontakt mit 0,1 m/s). Die
Karte zeigt Zeiten und Δv, der Δv-Balken die Kosten in Rosa. Beim Kontakt
werden die Schiffe ein Verbund („Frachter-1 + Frachter-3“): Massen, Tanks und
Triebwerke addieren sich, und der Verbund kann normal Manöver fliegen. Das
Ziel muss dafür auf einer stabilen Bahn sein und darf nichts mehr planen.

Abgedockt wird in der Karte „Verbund“: Teil wählen (◄ ►), mit − / + einstellen,
wie viel Treibstoff es mitnimmt (vorgeschlagen ist eine Aufteilung nach
Tankgröße, `Umschalt`: feine Schritte), darunter steht, was beide danach haben.
„Abdocken“ trennt sofort; das abgedockte Schiff bekommt 1 m/s gegen die
Flugrichtung und entfernt sich stetig (in 100 km Höhe etwa 6 km pro Umlauf).
Es hat danach das Hauptschiff als Ziel, zum Wiederandocken. So lässt sich zum
Beispiel ein Schiff im Orbit betanken.

### Monde und Planeten

Der Mond Zoon ist wie der Mun in KSP: 200 km Radius, Schwerkraft 1,63 m/s², keine
Luft. Er kreist in 12.000 km Abstand über dem Äquator (Umlauf etwa 38,6 h) und
zeigt dem Planeten immer dieselbe Seite. Um ihn liegt seine Einflusssphäre
(etwa 2.430 km, gestrichelter Kreis): Darin zählt nur seine Schwerkraft, außerhalb
nur die des Planeten (Patched Conics wie in KSP). Ein Klick auf den Mond wählt
ihn (Angaben links, Einflusssphäre gelb), `F` richtet die Kamera auf ihn.

Führt eine Bahn in die Einflusssphäre des Monds, wechselt sie dort auf eine
Bahn um ihn, beim Verlassen zurück zum Planeten, auch mitten in einem
Brennen. Gezeichnet wird wie in KSP: Das Stück um den Mond liegt um die
Stelle, an der er beim Eintritt stehen wird (dort ein blasser „Geistermond“),
kleine Ringe markieren Ein- und Austritt. Weil der Mond während des
Vorbeiflugs weiterzieht, klafft beim Austritt eine Lücke. Links steht die
nächste Begegnung („Begegnung: Zoon in 05:07:10, Periapsis 210,6 km“), im Flug
„Orbit um Zoon“; Nodes um den Mond gelten relativ zu ihm, und in der
Nahansicht liegt der Mond unten. Von der Parkbahn führen rund 835 m/s
prograd am Mond vorbei (Beispiel „Vorbeiflug an Zoon“); mit einem Brennen
gegen die Flugrichtung an der Periapsis bleibt das Schiff bei ihm. Gelandet wird auf Zoon wie auf Zorb über einen Node, nur
ohne Luftbremsung und Fallschirm: Das Landebrennen bremst die ganze
Geschwindigkeit, aus einer niedrigen Bahn mit viel Schub etwa 550 m/s. Mit
wenig Schub kostet es mehr (bei Schub/Gewicht 1,5 etwa 700 m/s): Es beginnt
höher, und die Schwerkraft zieht länger dagegen. Zielgebiete gibt
es auf der Nahseite (Zorb im Zenit) und der Fernseite. Zurück geht es mit
einem Start von Zoon (Voreinstellung siehe oben, die 30-km-Bahn kostet etwa
670 m/s).

Weiter draußen kreist der Mond Zill: klein, aber extrem dicht (100 km
Radius, Schwerkraft 5,75 m/s², keine Luft), auf einer deutlich elliptischen,
um 15° geneigten Bahn (35.250 bis 58.750 km vom Planeten, Umlauf knapp 50
Tage, Einflusssphäre etwa 9.050 km). Er ist das anspruchsvollere Ziel nach
Zoon: Landen und Wegkommen kosten mehr (Fluchtgeschwindigkeit am Boden rund
1.070 m/s, bei Zoon 810 m/s), und man muss den richtigen Punkt seiner Bahn
treffen. Hinflug und Landung kosten ab niedriger Bahn um Zorb rund 2.100 bis
2.300 m/s (Zoon: 1.760), der Rückflug rund 1.150: Das Basisschiff schafft
das direkt von Zorb nicht mehr sicher, der Frachter kommt mit Zorit nicht
ohne Tanken zurück. Dafür ist eine Tankstelle da (auf Zoon als
Zwischenstopp oder auf Zill selbst). Monde laufen wie in KSP auf festen
Keplerbahnen aus sechs Bahnelementen (große Halbachse, Exzentrizität,
Neigung, aufsteigender Knoten, Argument der Periapsis, mittlere Anomalie);
die Karte zeigt die Ellipse, die Mondkarte Abstand und Neigung. Weil Zills
Bahn geneigt ist, steht er meist weit über oder unter der Äquatorebene:
Ein einfacher Transfer erreicht seine Einflusssphäre, fliegt aber weit an
ihm vorbei (Beispiel „Zill“). Näher kommt man mit einer Ankunft nahe einem der
Knoten seiner Bahn oder mit einer Kurskorrektur unterwegs.

Zorb kreist wie Kerbin um die Sonne Zunn (13,6 Mio. km, ein Jahr hat 426
Tage). Seine Einflusssphäre reicht etwa 84.000 km weit; wer sie verlässt, ist
auf einer Bahn um Zunn und kann zu einem anderen Planeten fliegen, genauso
wie zu einem Mond. Nura ist Duna aus KSP nachempfunden: 320 km Radius,
Schwerkraft 2,94 m/s², dünne Luft bis 50 km, 20,7 Mio. km von Zunn
(leicht elliptisch), Einflusssphäre etwa 47.900 km. Ein Startfenster öffnet
sich, wenn Nura Zorb etwa 44° vorausläuft, alle gut 900 Tage; das erste
gleich zu Beginn. Der Flug kostet von der Parkbahn rund 1.090 m/s und dauert
etwa 270 Tage (Beispiel „Nura“). Zunn leuchtet als Kugel, das Licht kommt aus
ihrer Richtung, Tag- und Nachtseite der Planeten passen also. Die Karte
zoomt bis zum ganzen Sonnensystem heraus; `F` auf Zunn oder einen Planeten
(anklicken) zeigt ihn, Planeten bleiben aus der Ferne als Punkt sichtbar.

Für den Flug zu einem Planeten oder Mond wählt man ihn in der Karte „Ziel“
(mit ◄ ► nach den Schiffen). Sie zeigt den Phasenwinkel und wann das nächste
Startfenster kommt (ein Richtwert aus der Hohmann-Formel, für Nura etwa 44°),
dazu die engste Annäherung der geplanten Bahn an das Ziel, auch wenn sie
seine Einflusssphäre noch verfehlt; in der Karte markieren zwei Ringe mit
Linie die Stelle. So sieht man beim Ziehen am Node, ob es besser wird, bis
aus „Annäherung“ eine „Begegnung“ mit Periapsis wird.

### Fracht und Basen

Es gibt drei Stoffe:
Bauteile und Zorit (ein Erz, das es auf Zorb nicht gibt) sind Fracht und
teilen sich den Frachtraum, Treibstoff liegt im Tank. Fracht zählt als Masse
(weniger Δv, zu sehen am Δv-Balken). Alles läuft über die Karte „Fracht &
Basis" der Seitenleiste.

- **Startbasis:** Sie hat ein Lager für Zorit und Bauteile. Die Karte
  „Startbasis" (ein Schiff dort oder ihr Zielgebiet gewählt) zeigt es,
  tauscht Zorit gegen Bauteile (sie kommen ins Lager) und bestellt neue
  Schiffe, die sofort vollgetankt dort stehen. Schiffe in ihrem Zielgebiet
  laden Bauteile und Zorit aus dem Lager, laden beides dorthin ab und
  tanken. Tauschen und Laden sind zwei Schritte.
- **Preise in Zorit:** Basisschiff 1 t, Frachter 1 t, Bauteile 0,25 t je
  Tonne; Treibstoff ist an der Startbasis frei. Eine neue Basis kostet damit
  2 t Zorit.
- **Gründen:** Ein Basisschiff, das auf einem Körper außer Zorb gelandet ist,
  gründet dort eine **Mine** oder eine **Tankstelle** und wird ihr Fundament
  (es fliegt dann nicht mehr). Der Landepunkt wird zur Zone mit 5 km Radius,
  sichtbar wie jedes Zielgebiet, auch in der Vorschau der Landung.
- **Baustelle:** Sie braucht 4 t Bauteile; Frachter, die in der Zone landen,
  laden dort ab.
- **Tankstelle:** stellt Treibstoff her (180 kg pro Stunde, Lager 8 t);
  Schiffe in der Zone tanken.
- **Mine:** baut Zorit ab (100 kg pro Stunde); Frachter in der Zone laden es.
  Wie viel ihr Lager fasst, hängt vom Körper ab (Zoon 1 t, Zill 2 t, Nura
  4 t): Das ist die Menge pro Abholung. Weil Vorspulen nichts kostet, zählt
  der Ertrag pro Flug, nicht pro Stunde.

Frachter (2 t Frachtraum) und Basisschiff starten voll beladen von Zorb und
kommen bis Zoon; der Frachter kommt von dort ohne Tanken mit Zorit zurück
(auf Zorb bremsen Luft und Fallschirm). Zum schnellen Ausprobieren kreisen
in der Demo (`--demo`, siehe „Für Entwickler“) schon zwei Basisschiffe und zwei beladene Frachter um Zorb
(Beispiel „Vier Schiffe nach Zoon“).

### Aufgaben und Technik

Neben Zorit gibt es Daten (in GB). Sie gehören dem
Spieler, nicht einem Ort, und kommen von Aufgaben, die je einmal zählen: je
Körper eine niedrige, eine hohe und eine polare Umlaufbahn, außer auf Zorb
dazu eine Landung und eine fertige Basis (zusammen 290 GB). Orbitvorgaben
sind Bereiche, keine Punktlandungen: „niedrig" heißt um Zorb die ganze Bahn
zwischen 70 und 220 km, „hoch" ab 1.200 km, „polar" eine Bahnneigung von 80°
bis 100°. Eine Umlaufbahn zählt, wenn ein Brennen auf ihr endet; ein
Vorbeiflug zählt nicht, ein Absturz auch nicht. Die Karte „Manöver" zeigt
beim Node schon vorab, welche offenen Aufgaben die Bahn danach erfüllt
(„Erfüllt: …"). Eine Bahn darf mehrere Aufgaben zugleich erfüllen. Oben in
der Leiste steht der Stand („Daten 15 GB"); ein Klick öffnet das Fenster
„Wirtschaft" mit den Reitern Übersicht, Aufgaben und Technik.

Im Reiter „Technik" schaltet man mit Daten frei:
das Triebwerk Z-2 (30 GB; 3.600 statt 3.200 m/s Ausströmgeschwindigkeit),
das Chassis Groß (30 GB; 32 t Innenraum, 4 Triebwerksplätze), den Booster
B-2 (50 GB und 3 t Zorit; trägt Schiffe bis etwa 38 t statt 19 t) und das
Triebwerk Z-3 (100 GB, braucht Z-2; 4.000 m/s). Freischalten gilt sofort und
für immer; Chassis und Triebwerke sind dann im Editor wählbar. Der B-2
ersetzt den Booster der Startbasis endgültig (nur möglich, wenn er dort frei
steht). Er wirft kleine Schiffe steiler als der erste: Sie brauchen mit dem
Standardprofil etwas mehr eigenes Δv, ein größerer Wendewinkel gleicht das
aus.

### Zeit, Kamera und Anzeigen

Die Uhr oben zählt Tage von Zorb: Ein Tag ist ein Sonnentag (6 h, die
Stunden laufen von 0 bis 5), abgeleitet aus Drehung und Umlauf um Zunn; wie
Kerbin dreht sich Zorb dafür gegenüber den Sternen in 5:59:09. Diese eine
Zeit gilt überall, auch am Mond und auf Nura (wie die Kerbin-Zeit in KSP);
Zeitspannen wie „2 d 03:04:05“ zählen dieselben Tage.

Die Kamera lässt sich mit `F` auf das gewählte Objekt richten: Schiff,
Node, Zielgebiet (Raute in der Kreismitte anklicken) oder Mond. Sie folgt ihm, auch
wenn man danach etwas anderes wählt; so kann man ein Zielgebiet im Blick
behalten und dabei den Brems-Node des Schiffs einstellen. Mit `N` geht es in
die Nahansicht: Der Planet liegt unten, man kommt bis auf 10 m heran, zum
Beispiel an zwei Schiffe beim Rendezvous.

Ganz unten steht in der Statusleiste die letzte Meldung: Start, Brennbeginn
und Brennschluss (mit der Bahn danach), Eintritt in die Atmosphäre (mit der
Periapsis), Landebrennen, Landung (grün) und Absturz (rot, mit Grund). Gelb dazwischen
Hinweise der Bedienung, z. B. warum ein Node nicht geht. Ein Klick auf ▲
rechts klappt die letzten Meldungen auf, mit Mausrad und Scrollleiste. So
sieht man auch nach langem Zeitraffer, was passiert ist. Das Protokoll gehört
zum Spielstand.

## Steuerung

| Eingabe | Wirkung |
|---|---|
| Rechte Maustaste ziehen | Kamera drehen |
| Mausrad | Zoomen (über dem aufgeklappten Protokoll: scrollen) |
| ▲ / ▼ rechts in der Statusleiste | Protokoll auf- und zuklappen (mit Mausrad oder Scrollleiste blättern) |
| Linksklick auf ein Schiff (in der Karte oder in der Liste links) / `Tab` | Schiff wählen / nächstes Schiff |
| Schalter „Schiffe“ und „Info“ im Kopf der linken Leiste, „Hilfe“ oben rechts | Liste der Schiffe, Angaben zur Auswahl und die Bedienhilfe ein- und ausblenden (`F1`: Hilfe) |
| ◀ / ▶ im Kopf jeder Seitenleiste | Leiste ein- und ausklappen |
| Schalter „Entwürfe“ oben rechts | Schiffseditor: Entwürfe anlegen, ändern, löschen und bestellen |
| „Daten … GB“ oben rechts | Stand der Daten; Klick: Fenster „Wirtschaft“ mit drei Reitern: Übersicht (Basen mit Vorrat, Fracht an Bord; Klick auf eine Zeile wählt die Basis oder das Schiff), Aufgaben, Technik (mit Daten freischalten) |
| Klick auf einen Node in der Karte „Manöver“ | Node wählen |
| Regler unten in jeder Seitenleiste (Tooltip „Deckkraft“) | Wie durchscheinend diese Leiste ist (die Karte läuft dahinter weiter) |
| Linksklick auf die Raute in einem Zielgebiet | Zielgebiet wählen |
| Linksklick auf den Mond | Mond wählen (eine Bahn davor geht vor: Node setzen) |
| `F` | Kamera auf die Auswahl (Node, Schiff, Zielgebiet, Mond; ohne Auswahl Planet); nochmal `F` zurück zum Planeten. Ist ein Node gewählt, wechselt `F` der Reihe nach: Node, sein Schiff, Planet |
| `N` | Nahansicht an/aus (Planet unten, Zoom bis 10 m; nur mit einem Objekt im Fokus) |
| Seitenleiste „Start“ (gelandetes Schiff gewählt) | Start planen / abbrechen; −/+ ändern Startzeit, Kurs, Wendebeginn, Wendewinkel, Ziel-Apoapsis (`Umschalt`: feine Schritte); an der Startbasis mit Booster: zeigt Trennung, Boostback und Rückkehr |
| Seitenleiste „Booster“ (Booster gewählt) | Wo der Booster gerade ist und wann er zurück ist |
| Seitenleiste „Ziel“ ◄ ► | Ziel für ein Rendezvous wählen (anderes Schiff oder keins) |
| Seitenleiste „Ziel“: Andocken bei Annäherung | Andocken als letzten Schritt der Kette planen / abbrechen |
| Seitenleiste „Verbund“ | Teil wählen (◄ ►), Treibstoff zum Mitnehmen (− / +, `Umschalt`: fein), Abdocken |
| Seitenleiste „Startbasis“ (Schiff an der Startbasis oder ihr Zielgebiet gewählt) | Zorit im Lager, Preise; „Basisschiff bestellen“, „Frachter bestellen“ |
| Seitenleiste „Fracht & Basis“ (Frachter, Basisschiff oder Schiff an einer Basis gewählt) | Fracht, Baustelle und Lager; je nach Lage „Mine gründen“ / „Tankstelle gründen“, „Bauteile abladen“, „Tanken“, „Zorit laden“, an der Startbasis Bauteile und Zorit laden und abladen, „Tanken“ |
| Linksklick auf die Bahn des gewählten Schiffs | Node setzen (auch auf die Bahn nach einem geplanten Manöver) |
| Linksklick auf einen Node / `↑` `↓` | Node wählen (nur der gewählte hat Griffe) |
| Griff ziehen | Δv in diese Richtung ändern (je weiter, desto schneller; `Umschalt`: fein; höchstens bis der Tank leer ist) |
| Node ziehen | Node entlang seiner Bahn verschieben; spätere Nodes wandern mit. Liegen Mitte und ein Griff nah beieinander, greift der Klick, was näher am Mauszeiger ist; mit `Strg` sind die Griffe aus, man verschiebt nur. Auf einer Bahn durch die Luft kommt er nach innen gezogen auf einen späteren Umlauf („+N Umläufe“) |
| `Entf` / `Rücktaste` | gewählten Node löschen |
| `F5` / `F9` | Spielstand speichern / laden (siehe unten) |
| `H` | Zeitraffer-Stopp für das gewählte Schiff an/aus: Beginnt es zu brennen, hält der Zeitraffer genau dort an und schaltet auf 1× |
| `Esc` | Auswahl aufheben |
| `.` / `,` | Zeitraffer schneller / langsamer (1× bis 1 000 000×) |
| Leertaste | Pause |

Griffe: gelbgrün prograd/retrograd, violett normal/antinormal, türkis radial
nach außen/innen.

## Speichern und Laden

`F5` speichert, `F9` lädt. Es gibt einen Spielstand: die Datei
`spielstand.json` in dem Ordner, aus dem das Spiel gestartet wurde. Beim
Speichern bleibt der vorherige Stand als `spielstand.vorher.json` erhalten,
falls man einen guten Stand überschreibt (zum Zurückholen die Datei
umbenennen oder mit `--load spielstand.vorher.json` starten).

Gespeichert wird die ganze Welt: Zeit, Schiffe mit Bahnen und geplanten
Manövern, Fracht, Basen, Lager, Zorit und das Protokoll. Ein Flug geht nach
dem Laden exakt so weiter wie ohne Speichern. Kamera, Auswahl und Zeitraffer
gehören nicht dazu; nach dem Laden ist das Spiel pausiert. Es wird nie von
selbst gespeichert oder geladen: Das Spiel beginnt immer neu, `F9` holt den
Stand. Spielstände älterer Versionen des Spiels werden nicht geladen (eine
Meldung nennt die Versionen).

## Bilder

![Rendezvous](docs/screenshots/rendezvous.png)

*Frachter-3 zu Frachter-1 (rosa): Start im Startfenster auf dem Booster
(Kurs 125°, Neigung zum Ziel 0,0°), Node 1 macht die Bahn auf 100 km rund,
Node 2 hebt sie an, damit das Ziel aufholt. Danach kommt Frachter-3 ihm auf
418 m nahe (Ringe mit Linie). Nachspielen mit den Optionen unten.*

![Protokoll](docs/screenshots/protokoll.png)

*Nach 50 Minuten Spielzeit: Die aufgeklappte Statusleiste zeigt den Flug von
Frachter-3 in die Umlaufbahn (dafür gibt es 5 GB Daten), die Rückkehr des
Boosters zur Startbasis, ein Manöver von Frachter-1 und wie Frachter-2 bei
jedem Umlauf in die Luft taucht.*

![Andocken geplant](docs/screenshots/andocken-plan.png)

*Frachter-3 vor dem Andocken an Frachter-1: In 10 Minuten gleicht er an der
engsten Stelle (418 m) mit 133 m/s an, fliegt in 4 Minuten heran (8 m/s) und
hat danach noch knapp 4.200 m/s.*

![Andocken aus der Nähe](docs/screenshots/andocken.png)

*Im Anflug, aus der Nähe: Frachter-3 (gelb) ist noch 207 m von Frachter-1
(rosa) entfernt, Kontakt in zwei Minuten.*

![Mondflug geplant](docs/screenshots/mondflug-plan.png)

*Frachter-4 mit geplantem Transfer (`--node 4,600,835,0,0 --select 4
--camera 20,60,26000`): violett die Bahn zum Mond, unten links das Stück um
den Geistermond mit Ein- und Austritt, danach die neue Bahn um den Planeten.*

![Vorbeiflug](docs/screenshots/mondflug.png)

*Im Vorbeiflug (`--node 4,600,835,0,0 --time 20000 --select 4 --focus 4
--camera 120,70,7000`): Bahn um den Mond, Periapsis 211 km.*

![Start von Zoon](docs/screenshots/start-zoon.png)

*Frachter-4 nach der Landung im Zielgebiet Fernseite auf Zoon, Start geplant
mit der Voreinstellung ohne Luft (Wende ab 100 m mit 70°, Ziel 30 km).
Nachspielen mit `--demo --node 4,600,835,0,0 --node 4,24770,-300,0,0
--time 26700 --select 4 --focus 4 --near --camera 20,25,60` und „Start
planen“.*

![Sonnensystem](docs/screenshots/sonnensystem.png)

*Frachter-4 (gelb, unten) auf dem Weg nach Nura: Zunn in der Mitte, innen
Zorbs Bahn (die grüne Markierung sind Zorbs Zielgebiete), außen die von
Nura. Die blaue Bahn um die Sonne führt in Nuras Einflusssphäre (Ringe
rechts unten), links steht die Begegnung in 133 Tagen. Nachspielen mit
`--demo --node 4,163400,1088.5,0,0 --time 3000000 --select 4 --focus-body 0
--camera 30,60,64000000`.*

![Mond](docs/screenshots/mond.png)

*Der Mond ist gewählt: seine Bahn um den Planeten, die Einflusssphäre gelb,
links seine Angaben. Nachspielen mit `--demo --time 22100 --focus-body 2
--camera 75,50,7000` und einem Klick auf den Mond.*

![Startbasis](docs/screenshots/startbasis.png)

*Spielbeginn: Der bestellte Frachter-1 steht vollgetankt an der Startbasis,
in der Liste über ihm der Booster. Die Karte „Startbasis“ zeigt das Zorit im
Lager und die Preise, „Fracht & Basis“ den Preis für eine Ladung Bauteile.*

![Basis](docs/screenshots/basis.png)

*Zoon-Mine 1 ist fertig: Basisschiff-5 ist das Fundament, beide Frachter
haben abgeladen. Die Karte „Fracht & Basis“ zeigt das Lager der Mine; mit
„Zorit laden“ nimmt Frachter-8 mit, was da ist. Der grüne Kreis ist die Zone
der Basis (5 km).*

![Verbund](docs/screenshots/verbund.png)

*Nach dem Kontakt: Frachter-1 und Frachter-3 sind ein Verbund mit gemeinsamem
Tank. Die Karte „Verbund“ schlägt vor, wie viel Treibstoff Frachter-3 beim
Abdocken mitnimmt, und zeigt, was beide danach haben.*

![Nahansicht](docs/screenshots/nahansicht.png)

*Nahansicht beim Rendezvous aus dem Bild darüber: Kamera auf Frachter-3
(gelb), Frachter-1 (rosa) ist 942 m entfernt, die engste Annäherung kommt in
6 s.*

![Zielgebiet aus der Nähe](docs/screenshots/zielgebiet-nah.png)

*Kamera auf das Messfeld Ost in der Nahansicht, aus 12 km: Das Kreuz zeigt,
wo Frachter-1 nach dem geplanten Bremsen aufsetzt (`--demo --node
1,6375,-160,-40,0 --select 1 --focus-zone 2 --near --camera 20,35,12`).*

![Landung](docs/screenshots/landung.png)

*Dieselbe Landung aus dem Orbit, kurz vor dem Brennen (dazu `--time 6300
--camera 205,30,2800` statt der Nahansicht): Der Node bremst mit 160 m/s und
schiebt die Bahn mit 40 m/s zur Seite, damit sie über das Messfeld Ost
führt. Der Flug durch die Luft ist orange, das grüne Kreuz liegt im
Zielgebiet. Die Bahn ist ohne Drehung des Planeten gezeichnet, das Kreuz
zeigt den genauen Punkt auf der Oberfläche.*

![Startprofil](docs/screenshots/startprofil.png)

*Frachter-3 vor dem Start auf dem Booster: Aufstieg nach Osten (gelb), dazu
ein Node an der Apoapsis, der die Bahn auf rund 100 km rund macht. Rechts
die Karte „Start“ mit der Trennung und dem Rückflug des Boosters.*

![Manöver-Node](docs/screenshots/manoever-node.png)

*Rettung von Frachter-2: Node an der Apoapsis, 30 m/s prograd. Blau die
aktuelle Bahn, violett die Bahn danach.*

![Aufschlag](docs/screenshots/aufschlag.png)

*Frachter-2 bremst mit seinem letzten Treibstoff ab: Die Bahn führt durch die
Luft (orange) auf den Boden. Am Fallschirm kommt er mit 6 m/s an, für das
Landebrennen fehlt der Treibstoff; das rote Kreuz markiert den Absturz.*

![Manöverkette](docs/screenshots/manoeverkette.png)

*Hohmann-Übergang von 100 auf 500 km als Kette aus zwei Nodes: Der erste
(gewählt, mit Griffen) hebt die Apoapsis, der zweite liegt auf der violetten
Übergangsbahn an der Apoapsis und macht die Bahn rund.*

![Nächster Umlauf](docs/screenshots/naechster-umlauf.png)

*Node mit 250 m/s prograd, eine halbe Sekunde vor Frachter-1 gesetzt. Das
Brennen bräuchte knapp eine Sekunde Vorlauf, also liegt der Node einen
Umlauf später; das Etikett „+1 Umlauf“ zeigt es an.*

![Δv-Balken](docs/screenshots/dv-balken.png)

*Frachter-1 hat noch 1.077 m/s an Bord (von 6.039 m/s mit vollem Tank). Die
Kette will zweimal 600 m/s: Der zweite Node bekommt nur 477 m/s, 123 m/s
fehlen.*

## Für Entwickler

### Demo

Zum Ausprobieren und für alle Beispiele in diesem README,
die Schiffe mit Nummer nennen (`--node`, `--select`, `--launch`, `--target`
und so weiter), lädt `cargo run -p game -- --demo …` den Spielbeginn mit
weiteren Zielgebieten und mit Schiffen aus den beiden Startentwürfen, die
schon unterwegs sind. Die Nummer im Namen ist die Nummer des Schiffs für die
Optionen (der Booster ist Nummer 0). „Frachter-1" kreist stabil oberhalb der
Atmosphäre. „Frachter-2" wurde zu tief abgesetzt und ist fast leer: Er wird
bei jedem Durchgang durch die Luft abgebremst (orange) und kommt nach einigen
Umläufen herunter, außer man rettet ihn mit einem Manöver. „Frachter-3" steht
vollgetankt an der Startbasis, bereit zum Start auf dem Booster. „Frachter-4"
kreist in 100 km Höhe über dem Äquator, bereit für den Flug zum Mond.

### Optionen

```sh
cargo run -p game -- --demo --node 1,600,237,0,0 --node 1,2027,212,0,0 --camera -30,50,4200
```

- `--time SEKUNDEN`: bis zu dieser Spielzeit vorrechnen; Start, Nodes und Ziel
  gelten ab Spielbeginn und werden dabei ausgeführt
- `--camera GIER,NEIGUNG,KM`: Blickwinkel in Grad, Abstand vom Fokus in km
- `--focus SCHIFF` / `--focus-zone ZIELGEBIET`: Kamera auf ein Schiff bzw.
  Zielgebiet (0 = Startbasis, 1 = Übungsfeld, 2 = Messfeld Ost)
- `--focus-body KÖRPER`: Kamera auf einen Himmelskörper (0 = Zunn, 1 = Zorb,
  2 = Zoon, 3 = Zill, 4 = Nura)
- `--near`: Nahansicht (mit `--focus` oder `--focus-zone`)
- `--load DATEI`: einen Spielstand laden statt neu zu beginnen
- `--demo`: die Demo mit Schiffen laden (siehe oben)
- `--dock SCHIFF`: Andocken an das Ziel einschalten (nach `--target`)
- `--select SCHIFF`: Schiff wählen (0 = Booster; in der Demo ist die Nummer
  im Namen die Nummer des Schiffs, 3 = Frachter-3)
- `--launch SCHIFF,ZEIT,KURS,WENDE_KM,AP_KM[,WENDEWINKEL]`: Start planen
  (Zeitpunkt des Abhebens in s, Kurs und Wendewinkel in Grad; ohne
  Wendewinkel die Voreinstellung des Körpers), z. B. `--launch 3,120,90,1.5,100`
- `--node SCHIFF,ZEIT,PROGRAD,NORMAL,RADIAL[,SCHARF]`: Node setzen (Zeit in s,
  Δv in m/s), mehrfach für eine Kette; scharf wie im Spiel, `SCHARF` = 0 lässt
  die Kette zum Testen nicht scharf
- `--target SCHIFF,ZIEL`: Ziel für ein Rendezvous setzen
- `--target-body SCHIFF,KÖRPER`: Himmelskörper als Ziel setzen (Nummern wie
  bei `--focus-body`)
- `--screenshot DATEI.png`: Bild speichern und beenden (Zeit steht dabei still)

### Beispiele zum Nachspielen

Alle Zeilen gehören hinter `cargo run -p game -- --demo`.

- **Vorbeiflug an Zoon:** `--node 4,600,835,0,0`. Frachter-4 fliegt nach
  gut 5 Stunden in etwa 200 km Höhe am Mond vorbei.
- **Zill:** `--node 4,880,936,0,0`. Frachter-4 erreicht nach gut 4 Tagen
  Zills Einflusssphäre, fliegt aber in rund 6.600 km Höhe vorbei.
- **Nura:** `--node 4,163400,1088.5,0,0`. Frachter-4 verlässt Zorb und
  erreicht nach etwa 270 Tagen Nuras Einflusssphäre (Periapsis knapp
  10.000 km; mit einem Korrektur-Node unterwegs geht es tiefer). Mit
  `--select 4 --target-body 4,4 --focus-body 0 --camera 30,60,55000000`
  sieht man die Annäherung; mit 1.080 statt 1.088,5 m/s verfehlt die Bahn
  Nura um rund 109.000 km.
- **Vier Schiffe nach Zoon:** `--node 6,600,845,0,0 --node 7,613.5,845,0,0
  --node 8,602.5,845,0,0 --node 5,647.5,845,0,0`. Basisschiff-5 und die
  Frachter 7 und 8 (je 2 t Bauteile) landen nach knapp 8 Stunden
  nebeneinander bei 153° Ost, Basisschiff-6 bei 157° West. Mit `--time 28300`
  stehen sie schon dort.
- **Absturz:** `--node 2,600,-245,0,0`. Frachter-2 bremst mit seinem letzten
  Treibstoff ab und kommt am Fallschirm mit gut 6 m/s an, zu schnell für
  eine Landung.

Rendezvous Frachter-3 → Frachter-1 aus dem Bild oben (Start im Startfenster
auf dem Booster, Zirkularisieren, Aufholen):

```sh
cargo run -p game -- --demo --time 11100 --launch 3,11333,125,1.5,100 --target 3,1 \
  --node 3,11570,1579,0,0 --node 3,12550,124.85,0,0
```

Mit Andocken (`--dock 3`) und dann mitten im Anflug aus der Nähe (Bild
„Andocken aus der Nähe“):

```sh
cargo run -p game -- --demo --time 19760 --launch 3,11333,125,1.5,100 --target 3,1 \
  --node 3,11570,1579,0,0 --node 3,12550,124.85,0,0 --dock 3 --focus 3 --near --camera 40,20,0.6
```

Dasselbe ohne Andocken kurz vor der Annäherung in der Nahansicht (Bild „Nahansicht“):

```sh
cargo run -p game -- --demo --time 19627 --launch 3,11333,125,1.5,100 --target 3,1 \
  --node 3,11570,1579,0,0 --node 3,12550,124.85,0,0 --focus 3 --near --camera 50,25,6
```

### Tests

```sh
cargo test -p sim
```

Die Simulation hat keine Abhängigkeit zu Bevy, die Tests kompilieren und laufen
in Sekunden.

Jeder Pull Request wird automatisch geprüft (`.github/workflows/pruefung.yml`):
Formatierung, Clippy ohne Warnungen und alle Tests, erst die Simulation, dann
das Spiel. Dasselbe von Hand:

```sh
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test
```

### Aufbau

- `crates/sim`: die Simulation. Bahnen, Atmosphäre und Manöver; Start mit
  Booster, Landung, Rendezvous und Andocken; Fracht und Basen, Entwürfe aus
  dem Katalog, Aufgaben und Technik; Protokoll und Spielstand
- `crates/game`: Darstellung und Eingabe mit Bevy; die Oberfläche (Leisten
  oben, links und rechts, Fenster für Entwürfe und Wirtschaft, Δv-Balken,
  Statusleiste) mit egui
- `docs/ARCHITEKTUR.md`: Regeln, Physikmodell und Entscheidungen; am Ende
  die Ideen, die noch nicht gebaut sind
