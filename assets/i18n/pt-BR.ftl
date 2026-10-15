# Umber UI strings — pt-BR (Português do Brasil).
#
# Vocabulary follows pt-BR 3D-software usage, not literal translation:
# "Viewport", "Wireframe", "Bake" and "Assets" stay as the loanwords
# Brazilian artists use; "Malha" (mesh), "Camadas", "Grade" and "Grafo"
# are the established translations (Blender pt-BR). Labels are sentence
# case (pt-BR UI convention), not English title case. Keys missing here
# fall back to en-US.ftl.

## Barra de menus

menu =
    .file = Arquivo
    .edit = Editar
    .view = Exibir
    .help = Ajuda
    .open-mesh = Abrir malha…
    .open-project = Abrir projeto…
    .recent = Abrir recente
    .recent-empty = Nenhum projeto recente ainda.
    .recent-missing = Esta pasta não está acessível agora (movida, apagada ou em uma unidade desconectada).
    .recent-clear = Limpar recentes
    .revert = Reverter para o salvo
    .revert-unsaved = O projeto nunca foi salvo, então não há para onde reverter.
    .revert-clean = Nenhuma edição não salva para descartar.
    .revert-confirm-title = Reverter para o salvo?
    .revert-confirm-body = Isto descarta todas as edições desde o último salvamento e recarrega o projeto do disco.
    .window = Janela
    .panels = Painéis
    .reset-layout = Redefinir layout
    .about-copy = Copiar para a área de transferência
    .about-close = Fechar
    .save-project = Salvar projeto…
    .export-png = Exportar mapa pintado (PNG)…
    .load-environment = Carregar ambiente…
    .perf-hud = Mostrar HUD de desempenho
    .perf-hud-unavailable = O HUD de desempenho requer --features perf
    .wireframe = Mostrar wireframe (W)
    .grid = Mostrar grade (G)
    .gizmo = Mostrar gizmo de eixos
    .about = Sobre o Umber
    .shortcuts = Atalhos de Teclado
    .close-project = Fechar Projeto
    .show-log = Mostrar Log
    .show-log-failed = Não foi possível abrir o log { $path }: { $error }

## Títulos dos painéis

panel =
    .viewport = Viewport
    .uv-view = UV 2D
    .layers = Camadas
    .properties = Propriedades
    .assets = Assets
    .history = Histórico
    .texture-sets = Conjuntos de texturas
    .bakes = Bakes
    .export = Exportação
    .graph = Grafo
    .display = Exibição

## Botões principais

button =
    .bake = Fazer bake
    .export = Exportar
    .evaluate = Avaliar
    .save = Salvar
    .save-as = Salvar como…
    .reset = Redefinir
    .choose = Escolher…
    .add = Adicionar

## Mensagens de status

status =
    .baking = { $count ->
        [one] Fazendo bake de { $count } mapa…
       *[other] Fazendo bake de { $count } mapas…
    }
    .plugins = { $loaded ->
        [one] { $loaded } plugin carregado
       *[other] { $loaded } plugins carregados
    }, { $failed } com falha
    .uv = UV
    .tile = Tile
    .pen = Caneta
    .fps = fps

## Configurações

settings =
    .language = Idioma

## Fragmentos compartilhados

common =
    .none = nenhum
    .out-dir = Saída: { $dir }

## Shell do app: painéis provisórios, filtros dos diálogos de arquivo

shell =
    .assets-placeholder = Assets / prateleira (Onda 4+)
    .texture-sets-placeholder = Conjuntos de texturas (Onda 2)
    .filter-environment = Ambiente
    .filter-meshes = Malhas
    .new = Novo
    .new-confirm-title = Alterações não salvas
    .new-confirm-body = O projeto tem edições não salvas. Salvar primeiro, ou descartar e recomeçar?
    .new-confirm-save = Salvar…
    .new-confirm-discard = Descartar
    .new-confirm-cancel = Cancelar
    .hide-ui = Ocultar interface
    .maximize = Maximizar área
    .show-ui = Mostrar interface

    .tip-new = Começar um novo projeto vazio (não salvo; o atual permanece no disco se foi salvo)
    .open = Abrir…
    .tip-open-mesh = Carregar uma malha para pintar (ou soltar uma na viewport)
    .undo = Desfazer
    .redo = Refazer
    .wireframe = Wireframe
    .grid = Grade
    .view-lit = Iluminado
    .view-other = Sem luz / prévia
    .tip-view-cycle = Alternar entre sombreamento iluminado e sem luz (V)
    .mask = Máscara
    .invert-mask = Inverter
    .hero-title = Solte uma malha aqui para começar a pintar
    .hero-release = Solte para carregar a malha
    .hero-hint = ou use Abrir malha… na barra de ferramentas ou no menu Arquivo
    .hero-formats = Suportados:

## Painel de conjuntos de texturas

texturesets =
    .set = Conjunto de texturas
    .resize-confirm = Redimensionar { $set } de { $from } para { $to }? Isso apaga a tela do conjunto e o histórico de desfazer dos traços.
    .resize-apply = Redimensionar
    .resize-cancel = Cancelar
    .size = Tamanho do alvo de pintura
    .resize-clears = Mudar o tamanho do conjunto selecionado limpa a tela dele e o histórico de traços, e redefine sua máscara rápida. Isso é intencional.
    .no-paint = Sem sessão de pintura — o alvo da GPU está indisponível.

## Painel de bakes

bakes =
    .high-group = Alta resolução
    .high-none = Nenhuma malha de alta resolução
    .cage-front = Gaiola à frente
    .cage-back = Gaiola atrás
    .bake-from-high = Bake da alta
    .transfer-normal = Transferir normal
    .transfer-height = Transferir altura
    .map-ao = Oclusão de ambiente
    .map-curvature = Curvatura
    .map-thickness = Espessura
    .map-position = Posição
    .no-mesh = Nenhuma malha carregada — abra uma malha para fazer bake.
    .no-gpu = Sem dispositivo de GPU — o bake precisa do dispositivo wgpu.
    .resolution = Resolução
    .rays = Raios
    .dilation = Dilatação
    .margin = Tipo de margem
    .margin-extend = Estender
    .margin-adjacent = Faces adjacentes
    .margin-hint = Como a largura da dilatação é preenchida: Estender repete a borda de cada ilha; Faces adjacentes primeiro copia os texels através de cada costura UV a partir da face vizinha, depois estende o resto.
    .select-map = Selecione ao menos um mapa para o bake.
    .select-tile = Selecione ao menos um tile para o bake.
    .no-bake-yet = Nenhum bake ainda.
    .done = { $count ->
        [one] Bake de { $count } mapa concluído em { $ms } ms: { $files }
       *[other] Bake de { $count } mapas concluído em { $ms } ms: { $files }
    }
    .skipped-note = ignorados (a v1 só faz bake de AO por tile; os demais mapas só no tile 1001): { $pairs }
    .failed = Falha no bake: { $error }
    .worker-failed = Falha no bake: não foi possível iniciar o worker de bake: { $error }
    .map-skew = Skew (máscara de correção)
    .skew-threshold = Limite de skew
    .auto-rebake = Refazer o bake ao mudar
    .bake-all = Fazer bake de tudo
    .sets = Conjuntos de texturas:
    .select-set = Selecione ao menos um conjunto de texturas para o bake.
    .show-cage = Mostrar cage
    .log = Histórico de bakes
    .log-empty = Nenhum bake registrado ainda.
    .log-entry = [{ $time }] { $set } { $map } (tile { $tile }): { $ms } ms{ $warnings }
    .log-warnings = { $count ->
        [one] 1 texel com skew limitado
       *[other] { $count } texels com skew limitado
    }
    .skipped-skew-note = o skew só faz bake no tile 1001, tiles ignorados: { $tiles }
    .settings-group = Configurações
    .maps-group = Mapas

## Painel de exportação

export =
    .no-mesh = Nenhuma malha carregada — abra uma malha para exportar.
    .no-gpu = Sem dispositivo de GPU — a exportação precisa do dispositivo wgpu.
    .preset = Predefinição
    .size = Tamanho
    .materialx = MaterialX (.mtlx)
    .usda = USD (.usda)
    .select-tile = Selecione ao menos um tile para exportar.
    .tile-badge = { $count ->
        [one] { $count } tile
       *[other] { $count } tiles
    }
    .source-painted = Fonte da Base Color: pintura{ $tiles } — o que você pintou é o que será exportado.
    .source-graph = Fonte da Base Color: nó { $node } do grafo{ $tiles } — a saída avaliada do painel.
    .source-flat-size = Fonte da Base Color: cor sólida provisória — o alvo de pintura é { $width } × { $height }, não o tamanho de exportação { $size } × { $size }.
    .source-flat-none = Fonte da Base Color: cor sólida provisória — nenhuma sessão de pintura ou saída de grafo ativa.
    .base-painted = pintura
    .base-graph = nó { $node } do grafo
    .base-flat = cor sólida provisória
    .base-none = Base Color: nenhuma
    .base = Base Color: { $source }
    .base-tiles = Base Color: { $source } ({ $tiles })
    .no-export-yet = Nenhuma exportação ainda.
    .done = { $count ->
        [one] { $count } saída exportada ({ $base }; bake { $bake-ms } ms, gravação { $write-ms } ms): { $files }
       *[other] { $count } saídas exportadas ({ $base }; bake { $bake-ms } ms, gravação { $write-ms } ms): { $files }
    }
    .skipped-output = { $file } (faltando { $maps })
    .skipped = ignoradas: { $outputs }
    .tiles-skipped = tiles ignorados (sem geometria de malha): { $tiles }
    .failed = Falha na exportação: { $error }

## Painel do grafo (lista, editor, linha de status)

graph =
    .view-canvas = Canvas
    .view-list = Lista
    .canvas-help = Arraste o espaço vazio para mover a vista, use a roda para zoom, arraste uma porta de saída até uma entrada para conectar, clique com o botão direito para adicionar; Delete remove a aresta selecionada.
    .output = Saída: { $node }
    .nodes = Nós
    .no-nodes = Nenhum nó ainda — adicione um abaixo.
    .add-node = Adicionar nó
    .remove-selected = Remover selecionado
    .select-node = Selecione um nó para editar seus parâmetros.
    .node-title = Nó { $id }: { $def }
    .set-output = Definir como saída
    .edges = Arestas
    .edge = { $input } ← nó { $from }
    .edge-input = entrada
    .edge-from = de
    .add-edge = Adicionar aresta
    .anchor-layer = camada
    .anchor-channel = canal
    .anchor-none = sem camadas para vincular
    .no-output = Nenhuma saída ainda — clique em Avaliar.
    .uniform-output = Saída uniforme: { $value }
    .no-eval-yet = Nenhuma avaliação ainda.
    .loaded = Grafo carregado — clique em Avaliar.
    .loaded-unknown = { $count ->
        [one] Grafo carregado com { $count } tipo de nó desconhecido — clique em Avaliar.
       *[other] Grafo carregado com { $count } tipos de nó desconhecidos — clique em Avaliar.
    }
    .load-failed = Falha ao carregar o grafo: { $error }
    .empty = O grafo está vazio — adicione um nó primeiro.
    .no-output-node = Nenhum nó de saída — selecione um primeiro.
    .evaluated = { $count ->
        [one] { $count } nó avaliado em { $width }x{ $height }.
       *[other] { $count } nós avaliados em { $width }x{ $height }.
    }
    .eval-failed = Falha na avaliação: { $error }
    .save-material = Salvar como material
    .saved-material = Salvo como { $id } — edições futuras do grafo não propagam.
    .save-material-empty = Nada a salvar — o grafo está vazio.

## Canvas do grafo (editor de nós)

canvas =
    .empty-hint = Clique com o botão direito para adicionar um nó
    .add-node = Adicionar nó

## Nó de síntese de textura por exemplo (linha P1 do §4)

synth =
    .help = Síntese estocástica multirresolução a partir de uma imagem de exemplo. map_mode: 0 = Random Walk (preenche toda a tela), 1 = Inpaint (precisa de uma entrada `mask` conectada; os pixels não mascarados passam idênticos, byte a byte). tiling_mode: 0 = amostragem com recorte nas bordas, 1 = Tiled (amostragem com repetição, emenda perfeita nas bordas). randomness amplia o grupo de candidatos além do melhor único; seed torna o resultado reproduzível.

## Painel de exibição

display =
    .transform = Transformação
    .view = Vista
    .exposure = Exposição (EV)
    .gamma = Gama
    .preview = Prévia (rampa linear, 1 EV por amostra):
    .chain-identity = identidade (Raw, 0 EV, gama 1)
    .chain = exposição { $exposure } EV → { $view } → gama { $gamma }
    .live = Ativa no viewport 3D (passe da malha) e na vista UV (exibição da pintura) pela LUT de exibição da GPU.
    .rebuilding = Reconstruindo a LUT de exibição da GPU para o viewport 3D e a vista UV.
    .active-chain = Cadeia ativa: { $chain }. { $status }

## OCIO config adoption (Display panel section, §9 "in panel")
display-ocio =
    .title = OCIO / ACES
    .config = Configuração
    .space = Espaço de exibição
    .space-pick = Escolha um espaço de exibição
    .none = Apenas curvas embutidas
    .unavailable = OCIO indisponível nesta compilação — a biblioteca OCIO não foi compilada, então as configurações ACES 2.0 embutidas não podem carregar. As transformações fixas permanecem ativas.

## Pós-efeitos (seção do painel de exibição, §7 P1)

postfx =
    .title = Pós-efeitos
    .empty = Nenhum efeito — a cena vai direto para a transformação de exibição.
    .add = Adicionar efeito
    .dof = Profundidade de campo
    .bloom = Bloom
    .vignette = Vinheta
    .sharpen = Nitidez
    .tonemap = Mapeamento de tons
    .focus = Distância de foco
    .aperture = Abertura
    .threshold = Limiar
    .intensity = Intensidade
    .radius = Raio (px)
    .strength = Força
    .softness = Suavidade
    .amount = Quantidade
    .mode = Curva
    .exposure = Exposição (EV)
    .mode-aces = ACES (ajustada)
    .mode-filmic = Fílmica
    .mode-reinhard = Reinhard
    .drag-hint = Arraste para reordenar
    .move-up = Mover para cima
    .move-down = Mover para baixo
    .remove = Remover efeito
    .summary = { $active } de { $total } efeitos ativos, aplicados de cima para baixo.
    .status-idle = Nada ativo: o viewport 3D desenha direto, sem passe fora da tela.
    .status-live = Ativa no viewport 3D: a cena é renderizada em linear fora da tela, passa por esta cadeia e depois pela transformação de exibição.
    .status-bypassed = Ignorada no viewport 3D: o modo de exibição atual mostra dados brutos, que dispensam pós-efeitos e a transformação de exibição.
    .session-only = Só nesta sessão (não é salva no projeto).

## Modos de exibição do viewport (§5 P0)

viewmode =
    .label = Modo de exibição
    .lit = Iluminado (V)
    .unlit = Sem iluminação (V)
    .solo = Canal solo…
    .mesh-preview = Prévia de mapa da malha…
    .channel = Canal
    .map = Mapa
    .basecolor = Cor base
    .emissive = Emissivo
    .roughness = Rugosidade
    .metallic = Metálico
    .ao = Oclusão de ambiente
    .height = Altura
    .normal = Normal

## Painel de propriedades do pincel

brush =
    .no-preset = Nenhuma predefinição selecionada
    .cavity = Máscara de cavidade
    .cavity-enable = Pintar pela curvatura
    .cavity-cavities = Cavidades
    .cavity-ridges = Arestas
    .cavity-threshold = Limiar
    .cavity-width = Suavidade
    .cavity-invert = Inverter
    .cavity-no-map = Faça o bake do mapa de Curvatura para usar isto (painel Bakes).
    .stamped = Carimbo bitmap
    .stamp-angle = Ângulo do carimbo
    .angle-none = Sem rotação
    .angle-fixed = Fixo
    .angle-follow = Seguir o traço
    .angle-random = Aleatório
    .angle-degrees = Ângulo
    .angle-offset = Deslocamento
    .jitter-size = Variação de tamanho
    .jitter-angle = Variação de ângulo
    .jitter-position = Dispersão
    .jitter-flow = Variação de fluxo
    .stamp-image = Imagem do carimbo
    .stamp-none = Nenhuma (um quadrado cheio)
    .size = Tamanho
    .method = Método do traço
    .method-space = Espaçado
    .method-airbrush = Aerógrafo
    .method-dash = Tracejado
    .method-drag-dot = Ponto arrastado
    .method-rate = Taxa
    .method-dash-ratio = Proporção do traço
    .method-dash-length = Comprimento do traço
    .preset = Predefinição
    .preset-none = Nenhuma
    .broken = { $count ->
        [one] { $count } com defeito
       *[other] { $count } com defeito
    }
    .overridden = { $count ->
        [one] { $count } substituída
       *[other] { $count } substituídas
    }
    .name = Nome
    .color = Cor
    .alpha = Opacidade
    .hardness = Dureza
    .pressure-gamma = Gama da pressão
    .stabilizer = Estabilizador (one-euro)
    .min-cutoff = Corte mínimo
    .beta = Beta
    .d-cutoff = Corte d
    .lazy-mouse = Lazy mouse
    .radius-px = Raio (px)
    .strength = Intensidade
    .spacing = Espaçamento
    .dabs-per-radius = Dabs por raio
    .pressure-alpha = Pressão → opacidade
    .pressure-radius = Pressão → raio
    .curve-presets = Predefinições
    .curve-preset-linear = Linear
    .curve-preset-smooth = Suave
    .curve-preset-ease = Acelerada
    .wash-mode = Modo
    .wash = Lavar
    .build-up = Construir
    .falloff = Atenuação
    .falloff-smooth = Suave
    .falloff-smoother = Mais suave
    .falloff-sphere = Esfera
    .falloff-root = Raiz
    .falloff-sharp = Acentuada
    .falloff-sharper = Mais acentuada
    .falloff-linear = Linear
    .falloff-inverse-square = Quadrado inverso
    .falloff-constant = Constante
    .front-face-falloff = Atenuação de faces oblíquas
    .save-as-label = Salvar como
    .save-as-hint = Nome da nova predefinição
    .save-failed = Falha ao salvar: { $error }
    .save-as-needs-name = Salvar como precisa de um nome
    .save-as-needs-preset = Salvar como precisa de uma predefinição ativa
    .save-as-no-dir = Falha em Salvar como: sem pasta de predefinições do usuário
    .save-as-failed = Falha em Salvar como: { $error }
    .reset-failed = Falha ao redefinir: { $error }
    .tool = Ferramenta
    .tool-paint = Pintar
    .tool-smudge = Borrar
    .tool-clone = Clonar
    .tool-eraser = Borracha
    .tool-pick = Conta-gotas
    .tool-id-mask = Máscara de ID
    .erase-channels = Canais da borracha
    .erase-hint = Só o alfa por padrão (os dados de cor são mantidos, então desfazer os restaura). Ativar R/G/B apaga aquele canal de cor até zero mantendo o alfa.
    .smudge-strength = Intensidade
    .smudge-smear = Espalhamento
    .clone-offset = Deslocamento da origem (UV)
    .clone-follow = Seguir o traço
    .clone-hint = Alt+clique na vista UV 2D para definir a origem.
    .symmetry = Simetria
    .symmetry-mirror = Espelhar
    .symmetry-axis = Eixo
    .symmetry-axis-x = U (X)
    .symmetry-axis-y = V (Y)
    .symmetry-offset = Deslocamento (UV)
    .symmetry-hint = Arraste a linha do espelho na vista UV 2D para movê-la.
    .symmetry-radial = Radial
    .symmetry-count = Cópias
    .symmetry-flip-u = Inverter U
    .symmetry-flip-v = Inverter V
    .line-constraint = Linha reta
    .line-constraint-off = Desligada
    .line-constraint-line = Linha
    .line-constraint-snap = Snap de ângulo
    .line-constraint-angle = Ângulo do snap
    .line-constraint-hint = Ancorada no início do traço; o snap de ângulo trava a direção em múltiplos do ângulo.
    .projection = Projeção
    .projection-screen = 3D (pinte o que vê)
    .projection-uv = Somente UV
    .projection-hint = O 3D pinta através da malha sob o ponteiro (erros ignoram); somente UV orbita na viewport 3D.
    .quick-mask = Máscara rápida
    .quick-mask-mode = Modo máscara (X): os traços pintam a máscara
    .quick-mask-active = Restringir a pintura pela máscara
    .quick-mask-paint = Pintura da máscara
    .quick-mask-allow = Branco (permite)
    .quick-mask-block = Preto (bloqueia)
    .quick-mask-invert = Inverter (I)
    .quick-mask-clear = Limpar
    .quick-mask-inverted = invertida
    .quick-mask-hint = O vermelho marca os texels bloqueados. A máscara dura só esta sessão: não é salva nem pode ser desfeita.

## Painel de camadas

layers =
    .set-header = Conjunto de texturas: { $set }
    .add-paint = ＋ Adicionar camada de pintura
    .add-fill = ＋ Adicionar camada de preenchimento
    .masks-section = Máscaras
    .mask-add = ＋ Máscara
    .mask-remove = Remover máscara
    .mask-done = Concluir
    .mask-edit = Pintar máscara
    .default-paint-name = Pintura { $n }
    .default-fill-name = Preenchimento { $n }
    .kind-paint = pintura
    .kind-fill = preenchimento
    .kind-folder = pasta
    .kind-material = material
    .kind-instance = instância
    .instance-this = ⧉ Instância
    .instance-of = instância de { $source }
    .instance-name = { $source } (instância)
    .instance-paint-hint = Pinte a origem, não a instância.
    .add-material = ＋ Adicionar camada de material
    .default-material-name = Material { $n }
    .material-channels = Canais do material
    .material-id = Material
    .material-id-none = (sem material)
    .material-no-defs = Sem materiais — salve um grafo como material primeiro.
    .material-reset-channels = Redefinir canais
    .material-refresh = Atualizar material
    .material-channel-opacity = Opacidade
    .material-badge-no-params = Sem material — escolha abaixo.
    .material-badge-missing-def = Definição { $id } ausente — escolha outra ou salve de novo.
    .material-ignored = Ignora canais não declarados: { $channels }
    .material-unpaintable = Publica via refresh — os traços não o pintam.
    .fill-unpaintable = Publica por procedimento — os traços não o pintam. Pinte a máscara.
    .folder-unpaintable = Não tem pixels — selecione uma camada de pintura.
    .base-paint = Pintura base
    .base-paint-hint = Pintura que não pertence a nenhuma camada: projetos v1 e traços feitos sem camada selecionada.
    .budget-blocked = Orçamento de planos estourado ({ $bytes } bytes)
    .budget-line = Memória de planos { $used } / { $cap } bytes
    .stroke-refused = Traço recusado
    .tombstone-lost = Camada { $layer } recriada vazia ao desfazer (pixels aposentados liberados)
    .dormant-badge = Resultado pré-v2 vive na pintura base

## Importação de mapas (§5.6 — chaves novas; as demais caem no en-US)

import =
    .size-mismatch = Tamanho incompatível — camada vazia
    .budget-refused = Orçamento estourado na importação ({ $bytes } bytes) — camada vazia
    .decode-failed = Arquivo ilegível — camada vazia

## Propriedades da camada de preenchimento

fill =
    .section = Camadas de preenchimento
    .none = Sem camadas de preenchimento — adicione uma pelo painel Camadas.
    .proj-uv = UV
    .proj-triplanar = Triplanar
    .proj-planar = Planar
    .proj-spherical = Esférica
    .proj-cylindrical = Cilíndrica
    .axis = Eixo
    .axis-x = X
    .axis-y = Y
    .axis-z = Z
    .sharpness = Nitidez
    .scale = Escala
    .src-solid = Cor sólida
    .src-gradient = Gradiente
    .src-noise = Ruído
    .src-graph = Snapshot do grafo
    .alpha = Alfa
    .color-a = Cor A
    .color-b = Cor B
    .angle = Ângulo
    .seed = Semente
    .octaves = Oitavas
    .graph-note = Apenas um snapshot: reavalie o grafo para atualizar (a ligação viva com o grafo é um follow-up).
    .poly-tool = Preenchimento por polígono
    .poly-armed = Capturar cliques (laço)
    .poly-hint = Durante a captura, os cliques na vista UV 2D constroem a máscara em vez de pintar.
    .poly-mode-polygon = Polígono (laço)
    .poly-mode-triangle = Triângulo
    .poly-mode-mesh = Ilha da malha
    .poly-mode-uv-chunk = Ilha UV
    .poly-points = Pontos do laço
    .poly-pick = Triângulo escolhido
    .poly-no-pick = nenhum
    .poly-undo-point = Desfazer ponto
    .poly-clear = Limpar
    .poly-apply = Aplicar máscara
    .poly-clear-mask = Remover máscara

## Painel de histórico

history =
    .section-controls = Desfazer / refazer
    .section-log = Registro da sessão
    .undo = ↶ Desfazer
    .redo = ↷ Refazer
    .journal = { $count ->
        [one] { $count } entrada no histórico
       *[other] { $count } entradas no histórico
    }
    .empty = Nenhum histórico ainda.
    .op-add-paint = Adicionar camada de pintura
    .op-add-fill = Adicionar camada de preenchimento
    .op-add-folder = Adicionar camada de pasta
    .op-add-material = Adicionar camada de material
    .op-add-instance = Adicionar camada de instância
    .op-remove = Remover camada
    .op-reorder = Reordenar camada
    .op-set-opacity = Definir opacidade
    .op-set-visible = Definir visibilidade
    .op-set-blend = Definir modo de mesclagem
    .op-rename = Renomear camada
    .op-set-fill = Camada de preenchimento
    .op-set-material = Definir material
    .op-set-instance-source = Definir origem da instância
    .op-attach-mask = Anexar máscara
    .op-detach-mask = Remover máscara
    .op-set-mask-enabled = Alternar máscara
    .op-brush-stroke = Traço de pincel
    .op-bake = Bake
    .op-export = Exportação
    .strokes = Traços de borrar / clonar / borracha
    .undo-stroke = ↶ Desfazer traço
    .redo-stroke = ↷ Refazer traço
    .travel-note = Linhas de camada rebobinam camadas, linhas de traço rebobinam a pintura — cada uma por si.

## Seletor de tiles UDIM (Bakes + Exportação)

tiles =
    .label = Tiles:
    .tile = Tile { $tile }

## Combo de tamanho (Bakes + Exportação)

size =
    .vram-note = { $size } × { $size } (~{ $mib } MB/buffer de texels)

## Vista UV 2D
## PSD flatten-import (§7 row 101 — the composite-first honest flatten)

psd-import =
    .title = Importar PSD
    .hint = Solte um arquivo .psd do Photoshop. A composição achatada importa como um conjunto de texturas na resolução nativa; sem composição, o rasterizador honesto aplica camadas de pixel, opacidade de grupo e máscaras 1:1, e todo desvio entra no relatório de importação.
    .no-files = Nenhum PSD escaneado ainda.
    .scanned = { $matched } arquivo(s) PSD roteado(s), { $unmatched } ignorado(s).
    .apply = Importar PSD
    .report-title = Relatório de importação
    .row-matched = { $file } → conjunto “{ $set }”
    .row-unmatched = { $file } — { $reason }
    .reason-not-psd = não é um arquivo .psd

## .spp import (ruling 1 — import-only forever; the report is the artifact)

spp-import =
    .title = Importar .spp do Substance
    .hint = Solte um projeto .spp do Adobe Substance 3D Painter. O leitor importa os canais que consegue analisar com honestidade e relata cada conjunto de dados — grafos HBO são listados por era, não importados; a geometria continua FBX/USD.
    .no-files = Nenhum arquivo .spp escaneado ainda.
    .apply = Importar .spp
    .row-pending = { $file } aguardando
    .report-title = { $file } — Substance { $version }
    .set-row = Conjunto “{ $set }” — { $count } canal(is)
    .channel-row = { $channel } ({ $w}×{ $h}, { $c} componente(s))
    .note-row = { $note }


uv =
    .no-mesh = nenhuma malha carregada
    .mask-mode = MODO MÁSCARA (X para sair)

## Ferramentas de caminho de pintura (§2 P1)

path =
    .tool = Caminho de pintura
    .armed = Editar caminho (captura cliques)
    .hint = Durante a edição, os cliques na vista UV 2D constroem o caminho em vez de pintar. Arraste pontos para movê-los, clique num segmento para inserir, clique com o botão direito para excluir.
    .op-paint = Pintar
    .op-erase = Borracha
    .op-smudge = Borrar
    .closed = Laço fechado
    .points = Pontos
    .selected = Selecionado
    .size = Tamanho
    .opacity = Opacidade
    .replay = Repetir caminho
    .fill = Preencher caminho fechado
    .clear = Limpar caminho
    .no-paint = Sem sessão de pintura — repetir precisa do alvo da GPU.
    .need-points = Adicione ao menos dois pontos para repetir.

## Saída de superfície HDR (§5, linha P1 85)

display-hdr =
    .title = HDR
    .toggle = Saída HDR
    .reason-not-exposed = Ainda não disponível: a camada de janelas desta versão (eframe) não expõe o espaço de cor da superfície, então o umber não pode perguntar ao display se ele suporta HDR.
    .reason-sdr-only = Este adaptador ou superfície só ofereceu o espaço de cor SDR.

## Por que uma ação está indisponível (a verificação única do registro, plano G0)

action =
    .unavailable-nothing-to-undo = Nada para desfazer
    .unavailable-nothing-to-redo = Nada para refazer
    .unavailable-no-mesh = Carregue uma malha primeiro
    .unavailable-not-editing-mask = Só ao editar a máscara rápida (X)

## Dicas de mouse/teclado da barra de status (plano G1 g1-status-hints)

hint =
    .drop = Solte
    .straight-line = linha reta a partir do último traço
    .ctrl-shift = +Ctrl
    .snap-angle = ajustar a 5°
    .rmb-drag = Arrastar Dir
    .resize = tamanho / dureza
    .brackets = [ ]
    .size = tamanho do pincel
    .hud-size = Tamanho
    .hud-hardness = Dureza
    .alt-lmb = Alt+Esq
    .alt-mmb = Alt+Meio
    .alt-rmb = Alt+Dir
    .mmb = Meio
    .shift-mmb = Shift+Meio
    .load-mesh = uma malha para começar
    .ctrl-lmb = Ctrl+Esq
    .lmb-drag = Arrastar Esq
    .lmb = Esq
    .shift-lmb = Shift+Esq
    .wheel = Roda
    .ctrl-z = Ctrl+Z
    .paint = pintar
    .paint-stroke = pintar o traço
    .orbit = orbitar
    .pan = mover a vista
    .zoom = zoom
    .undo = desfazer
    .sample-color = amostrar a cor
    .select-region = selecionar a região

## Avisos toast (integração G0)

notice =
    .mesh-load-failed = Falha ao carregar a malha
    .mesh-upload-failed = Falha ao enviar a malha
    .environment-failed = Falha ao carregar o ambiente
    .export-failed = Falha na exportação
    .save-failed = Falha ao salvar o projeto
    .open-failed = Falha ao abrir o projeto

## Pixel autosave recovery (C2-3)

recovery =
    .title = Recuperar trabalho não salvo?
    .body = O Umber fechou sem salvar da última vez. Há uma cópia salva automaticamente desse trabalho (camadas e pintura).
    .recover = Recuperar

## Paleta de comandos (plano G3)

palette =
    .placeholder = Digite um comando…
    .path-mask = Máscara rápida
    .path-tools = Ferramentas
    .path-brush = Pincel
    .brush-smaller = Pincel menor
    .brush-larger = Pincel maior

## Preferências (plano G3)

prefs =
    .title = Preferências
    .search = Buscar configurações…
    .section-interface = Interface
    .ui-scale = Escala da interface
    .tooltips = Mostrar dicas
    .reduce-motion = Reduzir movimento
    .key-hints = Dicas de teclas na barra de status
    .section-navigation = Navegação
    .nav-preset = Navegação com o mouse
    .nav-painter = Painter (Alt + mouse)
    .nav-umber = Umber clássico
    .nav-blender = Blender (botão do meio)
    .nav-env-override = UMBER_NAV está definido: ele substitui esta escolha nesta execução.

## Dicas ricas (plano G3): só descrições, o registro imprime a tecla

tip =
    .hide-ui = Oculta os painéis, a barra de ferramentas e a barra de status: o viewport ocupa a janela. Tab de novo (ou o botão Mostrar interface) os traz de volta.
    .maximize = A área em foco ocupa a janela, mantendo o menu, a barra de ferramentas e a barra de status.
    .open-project = Abrir um projeto .umber salvo.
    .save-project = Salvar o projeto.
    .undo = Desfazer a última edição de camada.
    .redo = Refazer a última edição de camada desfeita.
    .wireframe = Mostrar o wireframe da malha sobre a pintura.
    .grid = Mostrar a grade do chão.
    .view-cycle-title = Modo de sombreamento
    .view-cycle = Alternar entre sombreamento iluminado e sem luz.
    .mask = Entrar ou sair do modo de máscara rápida: os traços pintam a máscara em vez da camada.
    .invert-mask = Inverter a máscara rápida.
    .palette-title = Paleta de comandos
    .palette = Encontrar e executar qualquer comando pelo nome.

## Espaços de trabalho (plano de GUI g5-workspaces)

workspace =
    .paint = Pintura
    .bake = Bake
    .export = Exportação
    .graph = Grafo
    .next = Próximo espaço de trabalho
    .prev = Espaço de trabalho anterior

## Teclas do modo de exibição (plano de GUI g6-display-mode-keys)

viewkeys =
    .material = Ver material
    .next-channel = Próximo canal
    .prev-channel = Canal anterior
    .next-map = Próximo mapa da malha
    .prev-map = Mapa da malha anterior
    .toast-material = Vendo o material
    .toast-unlit = Vendo sem iluminação
    .toast-channel = Canal:
    .toast-map = Mapa da malha:

## Menu de propriedade (plano de GUI g4-property-context-menu)

propmenu =
    .reset = Restaurar o padrão
    .copy = Copiar valor
    .paste = Colar valor
