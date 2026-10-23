## Sky, sun, moonlight and haze for a 3D scale, all driven by the cosmos sample (`env`):
## sun height from the sim, bearing from the hour (noon sun over the sea), moons trailing the
## sun by phase, clouds and flares into the sky shader, blue-green haze when the camera is under water.
extends Node3D

var world_env: WorldEnvironment
var environment: Environment
var sky_mat: ShaderMaterial
var sun: DirectionalLight3D
var moonlight: DirectionalLight3D

func _ready() -> void:
	sky_mat = ShaderMaterial.new()
	sky_mat.shader = load("res://shaders/sky.gdshader")
	var sky := Sky.new()
	sky.sky_material = sky_mat
	sky.radiance_size = Sky.RADIANCE_SIZE_128
	environment = Environment.new()
	environment.background_mode = Environment.BG_SKY
	environment.sky = sky
	environment.ambient_light_source = Environment.AMBIENT_SOURCE_SKY
	environment.ambient_light_energy = 0.35
	environment.reflected_light_source = Environment.REFLECTION_SOURCE_SKY
	environment.tonemap_mode = Environment.TONE_MAPPER_ACES
	environment.tonemap_exposure = 0.9
	environment.glow_enabled = true
	environment.glow_intensity = 0.6
	environment.glow_bloom = 0.08
	environment.glow_hdr_threshold = 1.1
	environment.ssao_enabled = true
	environment.ssao_radius = 2.0
	environment.ssao_intensity = 1.6
	environment.fog_enabled = true
	environment.fog_light_color = Color(0.55, 0.65, 0.78)
	environment.fog_density = 0.0007
	environment.fog_sky_affect = 0.15
	environment.adjustment_enabled = true
	environment.adjustment_saturation = 1.12
	world_env = WorldEnvironment.new()
	add_child(world_env)
	sun = DirectionalLight3D.new()
	sun.shadow_enabled = true
	sun.directional_shadow_max_distance = 420.0
	sun.light_angular_distance = 0.6
	add_child(sun)
	moonlight = DirectionalLight3D.new()
	moonlight.light_color = Color(0.55, 0.65, 0.95)
	moonlight.sky_mode = DirectionalLight3D.SKY_MODE_LIGHT_ONLY
	add_child(moonlight)

## Only one environment may be live: the active 3D scale's.
func set_active(on: bool) -> void:
	world_env.environment = environment if on else null

func update(env: Dictionary, cam_y: float, water_y: float) -> void:
	if env.is_empty():
		return
	var alt: float = asin(clamp(env.sun_height, -1.0, 1.0))
	var az: float = (env.day_phase - 0.5) * TAU
	var to_sun := Vector3(sin(az) * cos(alt), sin(alt), cos(az) * cos(alt)).normalized()
	_aim(sun, to_sun)
	var vis: float = env.visible
	sun.light_energy = clamp(vis * 1.2, 0.0, 1.3) if alt > -0.05 else 0.0
	var warm_c := Color(1.0, 0.6, 0.35).lerp(Color(1.0, 0.97, 0.9), clamp(alt / 0.5, 0.0, 1.0))
	sun.light_color = warm_c.lerp(Color(1.0, 0.75, 1.0), clamp(env.flare, 0.0, 1.0))
	sun.visible = alt > -0.05
	var night: float = clamp(-env.sun_height * 4.0 + 0.3, 0.0, 1.0)
	var m0 := moon_dir(env.moon0_phase, az)
	var m1 := moon_dir(env.moon1_phase, az)
	_aim(moonlight, m0 if m0.y > 0.0 else Vector3(0.2, 1.0, 0.3).normalized())
	moonlight.light_energy = 0.12 * night * (0.3 + 0.7 * (1.0 - abs(env.moon0_phase - 0.5) * 2.0))
	sky_mat.set_shader_parameter("cloud", env.cloud)
	sky_mat.set_shader_parameter("flare", env.flare)
	sky_mat.set_shader_parameter("time_days", env.t_days)
	sky_mat.set_shader_parameter("moon0_dir", m0)
	sky_mat.set_shader_parameter("moon1_dir", m1)
	sky_mat.set_shader_parameter("moon0_lit", 1.0 - abs(env.moon0_phase - 0.5) * 2.0)
	sky_mat.set_shader_parameter("moon1_lit", 1.0 - abs(env.moon1_phase - 0.5) * 2.0)
	environment.ambient_light_energy = 0.08 + 0.35 * clamp(vis * 1.5, 0.0, 1.0)
	var bright: float = clamp(vis * 1.6, 0.0, 1.0)
	if cam_y < water_y:
		# under water: thick blue-green haze, darker with depth and at night
		var depth: float = water_y - cam_y
		environment.fog_light_color = Color(0.01, 0.03, 0.05).lerp(Color(0.12, 0.38, 0.42), bright * exp(-depth * 0.03))
		environment.fog_density = 0.018
		environment.fog_sky_affect = 1.0
	else:
		environment.fog_light_color = Color(0.03, 0.04, 0.08).lerp(Color(0.6, 0.7, 0.82), bright)
		environment.fog_density = 0.0007
		environment.fog_sky_affect = 0.15

## A moon trails the sun by its phase (full moon opposite the sun).
static func moon_dir(phase: float, sun_az: float) -> Vector3:
	var az: float = sun_az + phase * TAU
	var alt: float = 0.9 * cos(az)
	return Vector3(sin(az) * cos(alt), sin(alt), cos(az) * cos(alt)).normalized()

func _aim(light: DirectionalLight3D, to: Vector3) -> void:
	var up := Vector3.UP if abs(to.y) < 0.99 else Vector3.FORWARD
	light.look_at_from_position(Vector3.ZERO, -to, up)
