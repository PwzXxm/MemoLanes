import 'package:flutter/services.dart' show rootBundle;
import 'package:memolanes/src/rust/api/achievement.dart' as achievement;

/// Load bundled display bytes only when the geometry cache needs them.
Future<achievement.RegionGeometry?> loadAchievementRegionGeometry(
  achievement.GeoEntityId id,
) => achievement.regionGeometry(
  entityId: id,
  loadAsset: () async {
    final data = await rootBundle.load('assets/geo/region_display.bin');
    return data.buffer.asUint8List(data.offsetInBytes, data.lengthInBytes);
  },
);
