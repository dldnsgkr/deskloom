import { sceneFactories } from './types';
import { rainWindowScene } from './rain-window';
import { weatherScene } from './weather';
import { hourglassScene } from './hourglass';
import { calendarScene } from './calendar';
import { noteScene } from './note';
import { walkerScene } from './walker';
import { widgetsAllScene } from './widgets-all';

sceneFactories['rain-window'] = rainWindowScene;
sceneFactories.weather = weatherScene;
sceneFactories.hourglass = hourglassScene;
sceneFactories.calendar = calendarScene;
sceneFactories.note = noteScene;
sceneFactories.walker = walkerScene;
sceneFactories['widgets-all'] = widgetsAllScene;
